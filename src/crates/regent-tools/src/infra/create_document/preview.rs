//! Optional background preview image of a generated document, for the vision
//! feedback loop (create → preview → `vision_analyze` → edit). Everything here is
//! HEADLESS — no window, no focus steal — so it is safe to run while the user is
//! on the machine:
//!   - PDF/report: screenshot the same report HTML via the renderer (Chromium
//!     `--headless=new`), so the preview is faithful to the PDF.
//!   - PPTX/deck: rasterize the deck's first slide via LibreOffice
//!     `soffice --headless`, in an isolated user profile so it never clashes with
//!     the user's own running LibreOffice. Skipped with a note when soffice is
//!     absent — we never fake a deck preview from an approximation.
//!
//! A preview failure is never fatal: the document is already written; the caller
//! surfaces the reason and moves on.

use super::model::DocumentSpec;
use super::theme::Theme;
use super::{html, renderer};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

const SOFFICE_TIMEOUT: Duration = Duration::from_secs(90);

/// `<document>.preview.png` beside the generated file.
fn preview_path(document: &Path) -> PathBuf {
    let mut name = document
        .file_name()
        .map(std::ffi::OsStr::to_owned)
        .unwrap_or_default();
    name.push(".preview.png");
    document.with_file_name(name)
}

async fn write_preview(document: &Path, png: &[u8]) -> Result<PathBuf, String> {
    let path = preview_path(document);
    tokio::fs::write(&path, png)
        .await
        .map_err(|error| format!("cannot write preview {}: {error}", path.display()))?;
    Ok(path)
}

/// Screenshot the report HTML (headless Chromium) into a preview PNG.
pub async fn preview_pdf(
    document: &Path,
    spec: &DocumentSpec,
    theme: &Theme,
) -> Result<PathBuf, String> {
    let report_html = html::report(spec, theme)?;
    let png = renderer::render(&json!({ "kind": "preview", "html": report_html })).await?;
    write_preview(document, &png).await
}

/// Rasterize the deck's first slide via headless LibreOffice into a preview PNG.
pub async fn preview_pptx(document: &Path) -> Result<PathBuf, String> {
    let soffice = find_soffice().ok_or_else(|| {
        "deck preview needs LibreOffice (headless) — install it or set REGENT_SOFFICE_PATH; \
         the deck itself was still created"
            .to_owned()
    })?;
    let png = soffice_first_slide_png(&soffice, document).await?;
    write_preview(document, &png).await
}

fn find_soffice() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("REGENT_SOFFICE_PATH") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }
    let candidates: &[&str] = if cfg!(windows) {
        &[
            r"C:\Program Files\LibreOffice\program\soffice.exe",
            r"C:\Program Files (x86)\LibreOffice\program\soffice.exe",
        ]
    } else if cfg!(target_os = "macos") {
        &["/Applications/LibreOffice.app/Contents/MacOS/soffice"]
    } else {
        &[
            "/usr/bin/soffice",
            "/usr/bin/libreoffice",
            "/snap/bin/libreoffice",
        ]
    };
    for candidate in candidates {
        if Path::new(candidate).exists() {
            return Some(PathBuf::from(candidate));
        }
    }
    let name = if cfg!(windows) {
        "soffice.exe"
    } else {
        "soffice"
    };
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(name))
            .find(|candidate| candidate.exists())
    })
}

async fn soffice_first_slide_png(soffice: &Path, document: &Path) -> Result<Vec<u8>, String> {
    let stem = document
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .ok_or("deck path has no file stem")?;
    // Convert into a private temp dir so we never overwrite a sibling <stem>.png,
    // and give LibreOffice an isolated profile so a headless run never collides
    // with the user's own open LibreOffice (they may be using the machine).
    let work = std::env::temp_dir().join(format!("regent-deck-preview-{}", std::process::id()));
    tokio::fs::create_dir_all(&work)
        .await
        .map_err(|error| format!("cannot create preview workdir: {error}"))?;
    let profile = file_url(&work.join("lo-profile"));

    let result = run_soffice(soffice, &profile, &work, document).await;
    // Best-effort cleanup regardless of outcome.
    let out = work.join(format!("{stem}.png"));
    let png = match &result {
        Ok(()) => tokio::fs::read(&out)
            .await
            .map_err(|error| format!("LibreOffice produced no preview: {error}")),
        Err(error) => Err(error.clone()),
    };
    tokio::fs::remove_dir_all(&work).await.ok();
    png
}

async fn run_soffice(
    soffice: &Path,
    profile_url: &str,
    outdir: &Path,
    document: &Path,
) -> Result<(), String> {
    let child = tokio::process::Command::new(soffice)
        .arg(format!("-env:UserInstallation={profile_url}"))
        .arg("--headless")
        .arg("--convert-to")
        .arg("png")
        .arg("--outdir")
        .arg(outdir)
        .arg(document)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot launch LibreOffice: {error}"))?;
    let output = tokio::time::timeout(SOFFICE_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| "LibreOffice preview timed out".to_owned())?
        .map_err(|error| format!("LibreOffice failed: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "LibreOffice exited {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(300)
                .collect::<String>()
        ))
    }
}

/// A `file://` URL for LibreOffice's `-env:UserInstallation` (needs forward
/// slashes; Windows drive paths become `file:///C:/...`).
fn file_url(path: &Path) -> String {
    let slashed = path.display().to_string().replace('\\', "/");
    if slashed.starts_with('/') {
        format!("file://{slashed}")
    } else {
        format!("file:///{slashed}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_path_appends_beside_the_document() {
        assert_eq!(
            preview_path(Path::new("/out/deck.pptx")),
            PathBuf::from("/out/deck.pptx.preview.png")
        );
    }

    #[test]
    fn file_url_is_forward_slashed() {
        assert!(file_url(Path::new("/tmp/x")).starts_with("file:///tmp/x"));
        // Windows-style path gains the triple slash before the drive.
        assert_eq!(file_url(Path::new(r"C:\a\b")), "file:///C:/a/b".to_owned());
    }
}

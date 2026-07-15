//! Real placement. The core stage runs the *same* `install.ps1`/`install.sh`
//! the one-liner uses, pointed at the bundled archive via REGENT_LOCAL_ARCHIVE
//! so it never touches the network. Everything the scripts already do (extract,
//! shim, PATH) stays there; this file only adds what a GUI install needs.

use crate::{log, InstallOptions};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;

/// Where `scripts/build-payload.*` staged the binaries.
fn payload_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let bundled = app
        .path()
        .resource_dir()
        .map_err(|e| format!("cannot locate resources: {e}"))?
        .join("payload");
    if bundled.is_dir() {
        return Ok(bundled);
    }
    // `tauri dev` does not stage bundle resources next to the binary, so a dev
    // run reads the payload straight out of the source tree.
    #[cfg(debug_assertions)]
    {
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("payload");
        if src.is_dir() {
            return Ok(src);
        }
    }
    Err("no bundled payload — run scripts/build-payload.ps1 (or .sh) first".into())
}

/// The release archive for this OS/arch. Exactly one is staged per build, so
/// take the first `regent-*.zip` / `regent-*.tar.gz` rather than re-deriving
/// the target triple here.
fn archive(payload: &Path) -> Result<PathBuf, String> {
    let entries = std::fs::read_dir(payload).map_err(|e| format!("read {payload:?}: {e}"))?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("regent-") && (name.ends_with(".zip") || name.ends_with(".tar.gz")) {
            return Ok(entry.path());
        }
    }
    Err(format!("no regent-*.zip/.tar.gz in {payload:?}"))
}

fn script(payload: &Path) -> PathBuf {
    payload.join(if cfg!(windows) {
        "install.ps1"
    } else {
        "install.sh"
    })
}

/// Deacon + CLI, via the bundled installer script in offline mode.
pub async fn core(app: &AppHandle, options: &InstallOptions) -> Result<(), String> {
    let payload = payload_dir(app)?;
    let archive = archive(&payload)?;
    let script = script(&payload);
    if !script.is_file() {
        return Err(format!("missing {script:?}"));
    }
    let bin_dir = Path::new(&options.install_dir).join("bin");

    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("powershell");
        c.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]);
        c.arg(&script);
        c
    } else {
        let mut c = Command::new("sh");
        c.arg(&script);
        c
    };
    cmd.env("REGENT_LOCAL_ARCHIVE", &archive)
        .env("REGENT_BIN_DIR", &bin_dir);
    if !options.add_to_path {
        cmd.env("REGENT_NO_PATH", "1");
    }
    stream(app, cmd).await
}

/// The desktop app: a bare executable, placed next to the core binaries.
pub async fn app_files(app: &AppHandle, options: &InstallOptions) -> Result<(), String> {
    let src = payload_dir(app)?.join("app");
    let dest = Path::new(&options.install_dir).join("app");
    std::fs::create_dir_all(&dest).map_err(|e| format!("create {dest:?}: {e}"))?;

    // build-payload stages a single executable here, so a flat copy is enough.
    let mut copied = 0;
    for entry in std::fs::read_dir(&src)
        .map_err(|e| format!("read {src:?}: {e}"))?
        .flatten()
    {
        if !entry.path().is_file() {
            continue;
        }
        let to = dest.join(entry.file_name());
        std::fs::copy(entry.path(), &to).map_err(|e| format!("copy to {to:?}: {e}"))?;
        log(app, format!("  {}", to.display()));
        copied += 1;
    }
    if copied == 0 {
        return Err(format!("no app executable in {src:?}"));
    }
    Ok(())
}

/// Run `cmd`, forwarding every stdout/stderr line to the live log.
async fn stream(app: &AppHandle, mut cmd: Command) -> Result<(), String> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run installer script: {e}"))?;

    // Both pipes are drained concurrently — reading one to completion first can
    // deadlock once the other fills its buffer.
    let out = child.stdout.take().map(|p| pump(app.clone(), p));
    let err = child.stderr.take().map(|p| pump(app.clone(), p));
    let (status, ..) = tokio::join!(child.wait(), maybe(out), maybe(err));

    match status.map_err(|e| format!("installer script: {e}"))? {
        s if s.success() => Ok(()),
        s => Err(format!("installer script failed ({s})")),
    }
}

async fn maybe(f: Option<impl Future<Output = ()>>) {
    if let Some(f) = f {
        f.await;
    }
}

async fn pump(app: AppHandle, pipe: impl AsyncRead + Unpin) {
    let mut lines = BufReader::new(pipe).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if !line.trim().is_empty() {
            log(&app, line);
        }
    }
}

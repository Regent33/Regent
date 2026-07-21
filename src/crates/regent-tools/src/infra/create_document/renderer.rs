//! Bridge to the Bun renderer sidecar (`regent __render`). We send one JSON job
//! on the child's stdin and read one JSON result — `{"ok":true,"bytes":"<b64>"}`
//! or `{"ok":false,"error":{...}}` — off stdout, decoding the bytes. Keeping
//! PptxGenJS and the headless-browser HTML→PDF path out-of-process avoids
//! bundling a browser or a Node runtime into the deacon.
//!
//! When no renderer is found the caller falls back to the native Rust writers,
//! so this never blocks document creation on a missing CLI/browser.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::Value;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

const RENDER_TIMEOUT: Duration = Duration::from_secs(90);

/// How to invoke the renderer: a program plus any fixed leading args (e.g.
/// `bun run <main.tsx>` in a dev checkout, or the compiled binary directly).
#[derive(Debug, Clone)]
pub struct RendererCmd {
    program: PathBuf,
    prefix: Vec<String>,
}

impl RendererCmd {
    /// Full argv (program first) — a pure, test-only view of what `command`
    /// runs, so command construction can be asserted without spawning.
    #[cfg(test)]
    fn argv(&self) -> Vec<String> {
        let mut argv = vec![self.program.to_string_lossy().into_owned()];
        argv.extend(self.prefix.iter().cloned());
        argv.push("__render".to_owned());
        argv
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.prefix).arg("__render");
        crate::infra::no_window::hide_tokio(&mut command);
        command
    }
}

/// Locate the renderer, or `None` (caller falls back to native writers):
/// `REGENT_CLI_PATH` → dev source (`bun run src/regent-cli/src/main.tsx`) →
/// compiled `regent-cli` next to a checkout → `regent` on PATH.
///
/// Dev source is preferred over the compiled binary so a checkout always renders
/// with the current renderer code, not a stale `dist/` build. In a deployed
/// layout there is no checkout and (usually) no bun, so it lands on the binary.
#[must_use]
pub fn find_renderer() -> Option<RendererCmd> {
    if let Ok(path) = std::env::var("REGENT_CLI_PATH") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(RendererCmd {
                program: path,
                prefix: Vec::new(),
            });
        }
    }

    if let Some(bun) = on_path(if cfg!(windows) { "bun.exe" } else { "bun" }) {
        for base in search_bases() {
            let main = base
                .join("src")
                .join("regent-cli")
                .join("src")
                .join("main.tsx");
            if main.exists() {
                return Some(RendererCmd {
                    program: bun,
                    prefix: vec!["run".to_owned(), main.to_string_lossy().into_owned()],
                });
            }
        }
    }

    let compiled = if cfg!(windows) {
        "regent-cli.exe"
    } else {
        "regent-cli"
    };
    for base in search_bases() {
        let candidate = base
            .join("src")
            .join("regent-cli")
            .join("dist")
            .join(compiled);
        if candidate.exists() {
            return Some(RendererCmd {
                program: candidate,
                prefix: Vec::new(),
            });
        }
    }

    if let Some(regent) = on_path(if cfg!(windows) {
        "regent.exe"
    } else {
        "regent"
    }) {
        return Some(RendererCmd {
            program: regent,
            prefix: Vec::new(),
        });
    }
    None
}

/// Ancestors of both the cwd and the current exe — where a checkout or an
/// installed layout might sit (mirrors `find_deacon`).
fn search_bases() -> Vec<PathBuf> {
    let mut bases: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        bases.extend(cwd.ancestors().map(PathBuf::from));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        bases.extend(dir.ancestors().map(PathBuf::from));
    }
    bases
}

fn on_path(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.exists())
}

/// Send `job` to the renderer and return the decoded output bytes. Errors from a
/// present-but-failing renderer are surfaced (never masked); a missing renderer
/// is signaled up-front by `find_renderer` returning `None`.
pub async fn render(job: &Value) -> Result<Vec<u8>, String> {
    let cmd = find_renderer().ok_or_else(|| {
        "document renderer unavailable (build it with `bun run compile` in src/regent-cli, \
         or set REGENT_CLI_PATH)"
            .to_owned()
    })?;
    let payload =
        serde_json::to_vec(job).map_err(|error| format!("cannot serialize render job: {error}"))?;

    let mut child = cmd
        .command()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot launch renderer {:?}: {error}", cmd.program))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(&payload)
            .await
            .map_err(|error| format!("renderer stdin write failed: {error}"))?;
        // Close stdin so the sidecar's read-to-EOF loop completes.
        stdin
            .shutdown()
            .await
            .map_err(|error| format!("renderer stdin close failed: {error}"))?;
    }

    let output = tokio::time::timeout(RENDER_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| "renderer timed out".to_owned())?
        .map_err(|error| format!("renderer process failed: {error}"))?;

    // The sidecar always emits a JSON result on stdout, even on a non-zero exit;
    // fall back to stderr only when stdout is not the expected JSON.
    match serde_json::from_slice::<Value>(&output.stdout) {
        Ok(value) => decode_render_response(&value),
        Err(_) => Err(format!(
            "renderer returned no JSON (exit {:?}): {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(500)
                .collect::<String>()
        )),
    }
}

/// Decode the sidecar's JSON result into bytes, or a message on `ok:false`.
pub fn decode_render_response(value: &Value) -> Result<Vec<u8>, String> {
    if value.get("ok").and_then(Value::as_bool) == Some(true) {
        let encoded = value
            .get("bytes")
            .and_then(Value::as_str)
            .ok_or_else(|| "renderer result missing `bytes`".to_owned())?;
        return BASE64
            .decode(encoded)
            .map_err(|error| format!("renderer bytes are not valid base64: {error}"));
    }
    let message = value
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("unknown renderer error");
    Err(format!("renderer error: {message}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn argv_appends_render_subcommand() {
        let dev = RendererCmd {
            program: PathBuf::from("bun"),
            prefix: vec!["run".into(), "main.tsx".into()],
        };
        assert_eq!(dev.argv(), vec!["bun", "run", "main.tsx", "__render"]);

        let binary = RendererCmd {
            program: PathBuf::from("/opt/regent-cli"),
            prefix: Vec::new(),
        };
        assert_eq!(binary.argv(), vec!["/opt/regent-cli", "__render"]);
    }

    #[test]
    fn decodes_ok_bytes() {
        let encoded = BASE64.encode(b"hello");
        let value = json!({ "ok": true, "bytes": encoded });
        assert_eq!(decode_render_response(&value).unwrap(), b"hello");
    }

    #[test]
    fn surfaces_error_message() {
        let value =
            json!({ "ok": false, "error": { "kind": "browser-missing", "message": "no browser" } });
        let error = decode_render_response(&value).unwrap_err();
        assert!(error.contains("no browser"), "{error}");
    }

    #[test]
    fn ok_without_bytes_is_an_error() {
        assert!(decode_render_response(&json!({ "ok": true })).is_err());
    }
}

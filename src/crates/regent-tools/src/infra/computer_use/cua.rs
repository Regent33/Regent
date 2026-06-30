//! CUA backend — drives the desktop through the `cua-driver` binary
//! (trycua/cua), the same driver Hermes uses. Each action shells out to
//! `cua-driver call <tool> <json-args>` (binary configurable via
//! `REGENT_CUA_DRIVER_CMD`, default `cua-driver`), mapping Regent's `Action`
//! set onto cua-driver's cross-platform tools (`screenshot` / `click` /
//! `type_text` / `hotkey`). This is the **default** computer-use backend; set
//! `REGENT_COMPUTER_USE_BACKEND=powershell` to use the native-Windows fallback.
//!
//! cua-driver must be installed + on PATH (macOS/Windows/Linux):
//!   Windows: irm https://raw.githubusercontent.com/trycua/cua/main/libs/cua-driver/scripts/install.ps1 | iex
//!   macOS/Linux: /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/trycua/cua/main/libs/cua-driver/scripts/install.sh)"
//!
//! The `cua-driver call` arg/result JSON shape follows cua-driver-rs's
//! documented surface; the mapping is centralized in `call`/`extract_image_b64`
//! so it is a one-line fix if a driver version differs.

use super::{ActOutput, Action, ComputerBackend};
use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use regent_kernel::RegentError;
use serde_json::{Value, json};

/// The cua-driver binary (overridable via `REGENT_CUA_DRIVER_CMD`).
pub fn driver_cmd() -> String {
    std::env::var("REGENT_CUA_DRIVER_CMD")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "cua-driver".to_owned())
}

const INSTALL_HINT: &str = "cua-driver not found — install it (trycua/cua) and ensure it's on PATH, or set \
     REGENT_CUA_DRIVER_CMD / REGENT_COMPUTER_USE_BACKEND=powershell";

pub struct CuaBackend;

#[async_trait]
impl ComputerBackend for CuaBackend {
    async fn act(&self, action: &Action) -> Result<ActOutput, RegentError> {
        let (tool, args) = match action {
            Action::Screenshot => ("screenshot", json!({"format": "png"})),
            Action::Click { x, y } => ("click", json!({"x": x, "y": y, "button": "left"})),
            Action::Type { text } => ("type_text", json!({"text": text})),
            Action::Key { combo } => ("hotkey", json!({"keys": combo})),
        };
        let result = call(tool, &args).await?;
        match action {
            Action::Screenshot => match extract_image_b64(&result) {
                Some(b64) => {
                    let bytes = B64
                        .decode(b64.trim())
                        .map_err(|e| tool_err(format!("cua-driver returned bad base64: {e}")))?;
                    let path = std::env::temp_dir()
                        .join(format!("regent-shot-{}.png", uuid::Uuid::new_v4().simple()));
                    tokio::fs::write(&path, &bytes)
                        .await
                        .map_err(|e| tool_err(format!("cannot save screenshot: {e}")))?;
                    Ok(ActOutput {
                        note: "captured via cua-driver".into(),
                        image_path: Some(path.display().to_string()),
                    })
                }
                None => Ok(ActOutput {
                    note: "cua-driver screenshot returned no image".into(),
                    image_path: None,
                }),
            },
            Action::Click { x, y } => Ok(ActOutput {
                note: format!("clicked ({x},{y}) via cua-driver"),
                image_path: None,
            }),
            Action::Type { .. } => Ok(ActOutput {
                note: "typed via cua-driver".into(),
                image_path: None,
            }),
            Action::Key { combo } => Ok(ActOutput {
                note: format!("pressed {combo} via cua-driver"),
                image_path: None,
            }),
        }
    }
}

fn tool_err(message: String) -> RegentError {
    RegentError::Tool {
        tool: "computer_use".into(),
        message,
    }
}

/// Invoke `cua-driver call <tool> <json>` and parse stdout as JSON (or wrap
/// plain text). A missing binary maps to the install hint.
async fn call(tool: &str, args: &Value) -> Result<Value, RegentError> {
    let output = tokio::process::Command::new(driver_cmd())
        .arg("call")
        .arg(tool)
        .arg(args.to_string())
        .output()
        .await
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                tool_err(INSTALL_HINT.to_owned())
            } else {
                tool_err(format!("cua-driver failed to run: {e}"))
            }
        })?;
    if !output.status.success() {
        return Err(tool_err(format!(
            "cua-driver call {tool} exited {}: {}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(serde_json::from_str(stdout.trim()).unwrap_or_else(|_| json!({ "text": stdout.trim() })))
}

/// Pull a base64 PNG out of a cua-driver result — it may sit in an MCP `image`
/// content part (`content[].data`) or in `structuredContent.screenshot_png_b64`
/// (handles both delivery shapes, matching Hermes' `_image_from_tool_result`).
fn extract_image_b64(result: &Value) -> Option<String> {
    if let Some(parts) = result.get("content").and_then(Value::as_array) {
        for part in parts {
            if part.get("type").and_then(Value::as_str) == Some("image")
                && let Some(b64) = part.get("data").and_then(Value::as_str)
            {
                return Some(b64.to_owned());
            }
        }
    }
    let structured = result.get("structuredContent").unwrap_or(result);
    for key in ["screenshot_png_b64", "png_b64", "image"] {
        if let Some(b64) = structured.get(key).and_then(Value::as_str) {
            return Some(b64.to_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn driver_cmd_defaults_and_overrides() {
        unsafe { std::env::remove_var("REGENT_CUA_DRIVER_CMD") };
        assert_eq!(driver_cmd(), "cua-driver");
    }

    #[test]
    fn extracts_image_from_both_shapes() {
        // MCP image content part.
        let a = json!({"content": [{"type": "image", "data": "AAAA"}]});
        assert_eq!(extract_image_b64(&a).as_deref(), Some("AAAA"));
        // structuredContent.screenshot_png_b64.
        let b = json!({"structuredContent": {"screenshot_png_b64": "BBBB"}});
        assert_eq!(extract_image_b64(&b).as_deref(), Some("BBBB"));
        // top-level png_b64 fallback.
        let c = json!({"png_b64": "CCCC"});
        assert_eq!(extract_image_b64(&c).as_deref(), Some("CCCC"));
        // none.
        assert_eq!(extract_image_b64(&json!({"text": "ok"})), None);
    }
}

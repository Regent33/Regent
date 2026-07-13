//! `control_app` — automate/drive desktop apps via an OS automation script:
//! PowerShell (incl. UI Automation / SendKeys) on Windows, AppleScript on
//! macOS, shell on Linux. This is powerful, so EVERY call is **approval-gated**
//! through the surface's `ApprovalHandler` (CLI prompt / Telegram `/approve`):
//! a denied or unattended call never runs. Distinct from `terminal` by intent
//! (GUI/app automation) and the always-on approval.

use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::io::AsyncWriteExt;

const TIMEOUT_SECS: u64 = 120;
const MAX_OUT_CHARS: usize = 16_000;

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "control_app".into(),
        description: "Automate or control a desktop app on this machine with an OS automation \
                      script — PowerShell (incl. UI Automation / SendKeys) on Windows, AppleScript \
                      on macOS, shell on Linux. Use to focus a window, send keystrokes, click menus, \
                      or script an app. Every call asks the user for approval first, so explain the \
                      intent in 'description'. (To merely launch an app or open a file/URL, use the \
                      terminal launcher instead.)"
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "script": {"type": "string", "description": "The automation script to run."},
                "lang": {
                    "type": "string",
                    "enum": ["powershell", "applescript", "shell"],
                    "description": "Script language; defaults to the host OS's native one."
                },
                "description": {
                    "type": "string",
                    "description": "Short human-readable summary of what this will do (shown for approval)."
                }
            },
            "required": ["script"]
        }),
        toolset: "app".into(),
    }
}

pub struct ControlAppTool;

#[async_trait]
impl ToolExecutor for ControlAppTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(script) = args
            .get("script")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
        else {
            return Ok(tool_error_json("missing required parameter: script"));
        };
        let lang = args
            .get("lang")
            .and_then(Value::as_str)
            .unwrap_or(default_lang());
        let summary = args
            .get("description")
            .and_then(Value::as_str)
            .filter(|d| !d.trim().is_empty())
            .map_or_else(|| first_line(script), str::to_owned);

        // Privilege gate: powerful, so always ask. Non-response resolves to Deny.
        let decision = ctx
            .approval
            .request("control_app", &summary, "desktop/app automation")
            .await;
        if decision.denied() {
            return Ok(tool_error_json(match decision.feedback() {
                Some(feedback) => format!("control_app denied: {feedback}"),
                None => "control_app denied by approval policy".to_owned(),
            }));
        }
        Ok(run_script(lang, script).await)
    }
}

fn default_lang() -> &'static str {
    if cfg!(windows) {
        "powershell"
    } else if cfg!(target_os = "macos") {
        "applescript"
    } else {
        "shell"
    }
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").chars().take(120).collect()
}

/// Run the script under the chosen interpreter with a timeout, capturing
/// stdout/stderr/exit. PowerShell/AppleScript go through a temp script file (no
/// quoting hazards); shell runs inline.
async fn run_script(lang: &str, script: &str) -> String {
    use tokio::process::Command;

    let mut temp: Option<std::path::PathBuf> = None;
    let mut command = match lang {
        "powershell" | "pwsh" => {
            let path = temp_path("ps1");
            if let Err(e) = write_temp(&path, script).await {
                return tool_error_json(e);
            }
            let mut c = Command::new("powershell");
            c.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
                .arg(&path);
            temp = Some(path);
            c
        }
        "applescript" | "osascript" => {
            let path = temp_path("scpt");
            if let Err(e) = write_temp(&path, script).await {
                return tool_error_json(e);
            }
            let mut c = Command::new("osascript");
            c.arg(&path);
            temp = Some(path);
            c
        }
        _ => {
            let mut c = Command::new("sh");
            c.arg("-c").arg(script);
            c
        }
    };

    let started = std::time::Instant::now();
    let result = tokio::time::timeout(Duration::from_secs(TIMEOUT_SECS), command.output()).await;
    if let Some(path) = temp {
        let _ = tokio::fs::remove_file(path).await;
    }

    match result {
        Err(_) => tool_error_json(format!("control_app timed out after {TIMEOUT_SECS}s")),
        Ok(Err(e)) => tool_error_json(format!("control_app failed to run ({lang}): {e}")),
        Ok(Ok(out)) => json!({
            "exit_code": out.status.code(),
            "stdout": truncate(&String::from_utf8_lossy(&out.stdout)),
            "stderr": truncate(&String::from_utf8_lossy(&out.stderr)),
            "duration_ms": started.elapsed().as_millis() as u64,
            "lang": lang,
        })
        .to_string(),
    }
}

fn temp_path(ext: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "regent-control-{}.{ext}",
        uuid::Uuid::new_v4().simple()
    ))
}

async fn write_temp(path: &std::path::Path, script: &str) -> Result<(), String> {
    let mut f = tokio::fs::File::create(path)
        .await
        .map_err(|e| e.to_string())?;
    f.write_all(script.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    f.flush().await.map_err(|e| e.to_string())?;
    Ok(())
}

fn truncate(text: &str) -> String {
    if text.chars().count() <= MAX_OUT_CHARS {
        return text.to_owned();
    }
    let head: String = text.chars().take(MAX_OUT_CHARS).collect();
    format!("{head}\n… [truncated at {MAX_OUT_CHARS} chars]")
}

#[cfg(test)]
#[path = "control_app_tests.rs"]
mod tests;

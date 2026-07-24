//! `computer_use` — coordinate-based desktop control (screenshot / click / type
//! / key), the Hermes `computer_use` gap. **High privilege**, so:
//!   * feature-flagged: only registered when `REGENT_COMPUTER_USE=1`,
//!   * approval-gated: every *mutating* action (click/type/key) goes through the
//!     surface's `ApprovalHandler`; `screenshot` is read-only and ungated,
//!   * backend behind a trait: the model drives a screenshot→action loop; this
//!     tool executes ONE action per call (the loop lives in the model).
//!
//! The default backend is [`CuaBackend`] (the cross-platform `cua-driver`, the
//! same driver Hermes uses); [`PowerShellBackend`] is a native-Windows fallback
//! (`REGENT_COMPUTER_USE_BACKEND=powershell`). Both sit behind
//! [`ComputerBackend`]. Screen content is **untrusted data** (§10.2) — the model
//! treats what it sees as input, never instructions.

mod contract;
mod cua;
mod powershell;
mod ps_scripts;
mod sendkeys;
pub use contract::{ActOutput, Action, ComputerBackend, definition};
pub use cua::CuaBackend;
pub use powershell::PowerShellBackend;

/// The configured backend. **CUA by default** (drives the cross-platform
/// `cua-driver`, the same driver Hermes uses); `REGENT_COMPUTER_USE_BACKEND=powershell`
/// selects the native-Windows fallback. With no explicit choice, a Windows
/// host without `cua-driver` on PATH falls back to PowerShell automatically —
/// a screenshot should never fail just because an optional driver is absent.
#[must_use]
pub fn default_backend() -> Arc<dyn ComputerBackend> {
    match std::env::var("REGENT_COMPUTER_USE_BACKEND").as_deref() {
        Ok("powershell") => Arc::new(PowerShellBackend),
        Ok(_) => Arc::new(CuaBackend),
        Err(_) if cfg!(windows) && !on_path(&cua::driver_cmd()) => Arc::new(PowerShellBackend),
        Err(_) => Arc::new(CuaBackend),
    }
}

/// Whether `cmd` resolves to an executable: an explicit path is checked
/// directly, a bare name is searched on PATH (with PATHEXT-style suffixes on
/// Windows).
fn on_path(cmd: &str) -> bool {
    let p = std::path::Path::new(cmd);
    if p.components().count() > 1 {
        return p.exists();
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    let suffixes: &[&str] = if cfg!(windows) {
        &["", ".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    std::env::split_paths(&paths).any(|dir| {
        suffixes
            .iter()
            .any(|s| dir.join(format!("{cmd}{s}")).exists())
    })
}

use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use crate::infra::vision_analyze::VisionAnalyzeTool;
use async_trait::async_trait;
use regent_kernel::{RegentError, tool_error_json};
use serde_json::{Value, json};
use std::sync::Arc;

/// `true` when computer-use is enabled (`REGENT_COMPUTER_USE=1`).
#[must_use]
pub fn is_enabled() -> bool {
    std::env::var("REGENT_COMPUTER_USE").as_deref() == Ok("1")
}

pub struct ComputerUseTool {
    backend: Arc<dyn ComputerBackend>,
}

impl ComputerUseTool {
    #[must_use]
    pub fn new(backend: Arc<dyn ComputerBackend>) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl ToolExecutor for ComputerUseTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        if !is_enabled() {
            return Ok(tool_error_json(
                "computer_use is disabled — set REGENT_COMPUTER_USE=1 to enable it",
            ));
        }
        let action = match parse_action(&args) {
            Ok(action) => action,
            Err(error) => return Ok(tool_error_json(error)),
        };
        if let Action::Key { combo } = &action
            && is_blind_close_combo(combo)
        {
            return Ok(tool_error_json(
                "blind close shortcut blocked because it can close Regent or the wrong focused \
                 app; call list_windows then close_window, or list_tabs then close_tab",
            ));
        }
        // Privilege gate: mutating actions always ask. Non-response → Deny.
        if action.is_mutating() {
            let decision = ctx
                .approval
                .request("computer_use", &action.label(), "desktop control")
                .await;
            if decision.denied() {
                return Ok(tool_error_json(match decision.feedback() {
                    Some(feedback) => format!("computer_use denied: {feedback}"),
                    None => "computer_use denied by approval policy".to_owned(),
                }));
            }
        }
        match self.backend.act(&action).await {
            Ok(out) => {
                let vision = if matches!(action, Action::Screenshot) {
                    match (
                        out.image_path.as_deref(),
                        args.get("question")
                            .and_then(Value::as_str)
                            .filter(|question| !question.trim().is_empty()),
                    ) {
                        (Some(path), Some(question)) => {
                            let raw = VisionAnalyzeTool
                                .execute(json!({"image_url": path, "question": question}), ctx)
                                .await?;
                            serde_json::from_str::<Value>(&raw).unwrap_or(Value::String(raw))
                        }
                        _ => Value::Null,
                    }
                } else {
                    Value::Null
                };
                let ok = vision_succeeded(&vision);
                Ok(json!({
                    "ok": ok,
                    "action": action.label(),
                    "note": out.note,
                    "image_path": out.image_path,
                    "vision": vision,
                })
                .to_string())
            }
            Err(error) => Ok(tool_error_json(format!("computer_use failed: {error}"))),
        }
    }
}

fn is_blind_close_combo(combo: &str) -> bool {
    let normalized = combo
        .chars()
        .filter(|character| !character.is_ascii_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "alt+f4" | "ctrl+w" | "control+w" | "cmd+w" | "cmd+q" | "command+w" | "command+q"
    )
}

fn vision_succeeded(vision: &Value) -> bool {
    let Some(object) = vision.as_object() else {
        return true;
    };
    if object.contains_key("error") {
        return false;
    }
    object
        .get("success")
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

use parse::parse_action;

mod parse;

#[cfg(test)]
#[path = "tests/parse.rs"]
mod parse_tests;
#[cfg(test)]
mod tests;

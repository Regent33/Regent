use crate::domain::contracts::{ApprovalDecision, TerminalBackend, ToolExecutor};
use crate::domain::entities::ToolContext;
use crate::domain::guard::detect_dangerous_command;
use crate::infra::backends::LocalBackend;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

const DEFAULT_TIMEOUT_SECS: u64 = 60;
const MAX_TIMEOUT_SECS: u64 = 600;
const MAX_STREAM_CHARS: usize = 24_000;

// How to launch a desktop app / open a file or URL, per OS — surfaced in the
// tool description so the agent knows it *can* open things on the machine.
#[cfg(windows)]
const LAUNCH_HINT: &str = "`start <app>` — e.g. `start chrome`, `start notepad`, or `start \"\" \"<path>\"` for a file/URL";
#[cfg(target_os = "macos")]
const LAUNCH_HINT: &str = "`open <app/file/url>` — e.g. `open -a Safari` or `open ~/file.pdf`";
#[cfg(all(not(windows), not(target_os = "macos")))]
const LAUNCH_HINT: &str = "`xdg-open <file/url>` (or run the app's binary directly)";

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "terminal".into(),
        description: format!(
            "Run a shell command and return its exit code, stdout, and stderr. Commands run in \
             the session working directory unless cwd is given. To open a desktop app, file, or \
             URL on this machine, use the OS launcher: {LAUNCH_HINT}."
        ),
        parameters: json!({
            "type": "object",
            "properties": {
                "command": {"type": "string", "description": "The command to run."},
                "cwd": {"type": "string", "description": "Working directory (optional)."},
                "timeout_secs": {"type": "integer", "description": "Kill after N seconds (default 60, max 600)."}
            },
            "required": ["command"]
        }),
        toolset: "terminal".into(),
    }
}

/// The terminal tool: guard + approval + truncation live here; execution is
/// the backend's job (local by default; docker/ssh via `backends`).
pub struct TerminalTool {
    backend: Arc<dyn TerminalBackend>,
}

impl Default for TerminalTool {
    fn default() -> Self {
        Self {
            backend: Arc::new(LocalBackend),
        }
    }
}

impl TerminalTool {
    #[must_use]
    pub fn with_backend(backend: Arc<dyn TerminalBackend>) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl ToolExecutor for TerminalTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(command) = args.get("command").and_then(Value::as_str) else {
            return Ok(tool_error_json("missing required parameter: command"));
        };
        if invokes_regent_cli(command) {
            // The agent IS the running daemon. Spawning the `regent` CLI here boots
            // a SECOND daemon that deadlocks on the shared SQLite store — the
            // "terminal hit a snag" the user saw. Return guidance instead of hanging.
            return Ok(tool_error_json(
                "You ARE the running Regent daemon — running the `regent` CLI from \
                 your terminal would spawn a second daemon that deadlocks on the \
                 shared database (that is the 'snag'). Do the task with your own \
                 tools instead (manage_keys, update_persona, kanban, memory, skills, \
                 files, web), or tell the user the exact `regent <command>` (or \
                 in-chat `/<command>`) to run themselves.",
            ));
        }
        if let Some(reason) = detect_dangerous_command(command) {
            let decision = ctx.approval.request("terminal", command, reason).await;
            if decision == ApprovalDecision::Deny {
                return Ok(tool_error_json(format!(
                    "command denied by approval policy ({reason})"
                )));
            }
        }
        let cwd = match args.get("cwd").and_then(Value::as_str) {
            Some(p) => match ctx.resolve(p) {
                Ok(resolved) => resolved,
                Err(error) => return Ok(tool_error_json(error.to_string())),
            },
            None => ctx.cwd.clone(),
        };
        let timeout_secs = args
            .get("timeout_secs")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_TIMEOUT_SECS)
            .min(MAX_TIMEOUT_SECS);

        let started = std::time::Instant::now();
        match self
            .backend
            .run(command, &cwd, Duration::from_secs(timeout_secs))
            .await
        {
            Err(error) => Ok(tool_error_json(error.to_string())),
            Ok(output) => Ok(json!({
                "exit_code": output.exit_code,
                "stdout": truncate_stream(&output.stdout),
                "stderr": truncate_stream(&output.stderr),
                "duration_ms": started.elapsed().as_millis() as u64,
                "backend": self.backend.describe(),
            })
            .to_string()),
        }
    }
}

/// Whether `command` invokes the `regent` CLI — as the first word of the command
/// or of any `&&`/`||`/`|`/`;`/newline-separated segment. The agent is the daemon,
/// so this would deadlock a second daemon on the shared store; we short-circuit it.
fn invokes_regent_cli(command: &str) -> bool {
    command
        .split([';', '\n', '|', '&'])
        .map(str::trim)
        .filter_map(|seg| seg.split_whitespace().next())
        .any(|first| {
            let token = first.trim_matches(|c| c == '"' || c == '\'');
            let name = token.rsplit(['/', '\\']).next().unwrap_or(token);
            name.eq_ignore_ascii_case("regent") || name.eq_ignore_ascii_case("regent.exe")
        })
}

fn truncate_stream(text: &str) -> String {
    if text.chars().count() <= MAX_STREAM_CHARS {
        return text.to_owned();
    }
    let truncated: String = text.chars().take(MAX_STREAM_CHARS).collect();
    format!("{truncated}\n… [output truncated at {MAX_STREAM_CHARS} chars]")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::contracts::{ApprovalHandler, DenyAll};
    use std::sync::atomic::{AtomicBool, Ordering};

    fn ctx_with(approval: Arc<dyn ApprovalHandler>) -> ToolContext {
        ToolContext::new(std::env::temp_dir(), approval)
    }

    #[tokio::test]
    async fn echo_round_trip() {
        let out = TerminalTool::default()
            .execute(
                json!({"command": "echo regent-core"}),
                &ctx_with(Arc::new(DenyAll)),
            )
            .await
            .unwrap();
        let value: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(value["exit_code"], 0);
        assert_eq!(value["backend"], "local");
        assert!(value["stdout"].as_str().unwrap().contains("regent-core"));
    }

    #[test]
    fn detects_regent_cli_invocations() {
        assert!(invokes_regent_cli("regent status"));
        assert!(invokes_regent_cli("  regent model set claude-opus-4-8"));
        assert!(invokes_regent_cli("cd foo && regent kanban list"));
        assert!(invokes_regent_cli("echo hi; regent.exe status"));
        assert!(invokes_regent_cli("ls | regent status"));
        // Not the CLI: `regent` only as an argument or substring.
        assert!(!invokes_regent_cli("echo regent is great"));
        assert!(!invokes_regent_cli("git commit -m 'regent'"));
        assert!(!invokes_regent_cli("cat regent.txt"));
    }

    #[tokio::test]
    async fn regent_cli_command_is_short_circuited() {
        let out = TerminalTool::default()
            .execute(json!({"command": "regent status"}), &ctx_with(Arc::new(DenyAll)))
            .await
            .unwrap();
        assert!(out.contains("running Regent daemon"), "got: {out}");
    }

    #[tokio::test]
    async fn dangerous_command_is_denied_without_approval() {
        struct Recorder(AtomicBool);
        #[async_trait]
        impl ApprovalHandler for Recorder {
            async fn request(&self, _: &str, _: &str, _: &str) -> ApprovalDecision {
                self.0.store(true, Ordering::SeqCst);
                ApprovalDecision::Deny
            }
        }
        let recorder = Arc::new(Recorder(AtomicBool::new(false)));
        let out = TerminalTool::default()
            .execute(json!({"command": "rm -rf /"}), &ctx_with(recorder.clone()))
            .await
            .unwrap();
        assert!(out.contains("denied by approval policy"));
        assert!(
            recorder.0.load(Ordering::SeqCst),
            "approval gate must be consulted"
        );
    }

    #[tokio::test]
    async fn timeout_kills_and_reports() {
        let command = if cfg!(windows) {
            "ping -n 30 127.0.0.1 > NUL"
        } else {
            "sleep 30"
        };
        let out = TerminalTool::default()
            .execute(
                json!({"command": command, "timeout_secs": 1}),
                &ctx_with(Arc::new(DenyAll)),
            )
            .await
            .unwrap();
        assert!(out.contains("timed out"));
    }
}

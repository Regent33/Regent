//! Lifecycle shell hooks (gap S7): user-configured commands spawned at the
//! tool-dispatch seams. Observe-only and fire-and-forget — a hook can log,
//! notify, or kick a build, but it cannot block or veto the dispatch.
// ponytail: blocking pre-hooks would need an async DispatchHook seam; gating
// is already covered by permission rules (gap S5) — add blocking only if a
// real hook needs a veto the rules can't express.

use crate::domain::contracts::DispatchHook;
use serde_json::Value;

/// Payload cap for `REGENT_HOOK_PAYLOAD` — hooks get a summary, not a stream.
const PAYLOAD_MAX_CHARS: usize = 2_000;

/// Spawns configured shell commands on dispatch events. The command sees:
/// `REGENT_HOOK_EVENT` (`tool_start` | `tool_complete`), `REGENT_HOOK_TOOL`,
/// and `REGENT_HOOK_PAYLOAD` (args or result, truncated).
pub struct ShellHook {
    on_start: Option<String>,
    on_complete: Option<String>,
}

impl ShellHook {
    /// Empty strings mean "no hook for this event".
    #[must_use]
    pub fn new(on_start: &str, on_complete: &str) -> Self {
        let some = |s: &str| {
            let t = s.trim();
            (!t.is_empty()).then(|| t.to_owned())
        };
        Self {
            on_start: some(on_start),
            on_complete: some(on_complete),
        }
    }

    /// True when at least one event has a command (callers can skip
    /// registering an inert hook).
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.on_start.is_some() || self.on_complete.is_some()
    }

    fn fire(command: Option<&str>, event: &str, tool: &str, payload: &str) {
        let Some(command) = command else { return };
        let clipped: String = payload.chars().take(PAYLOAD_MAX_CHARS).collect();
        #[cfg(windows)]
        let mut cmd = {
            use std::os::windows::process::CommandExt;
            let mut c = std::process::Command::new("cmd");
            // raw_arg: hand cmd.exe the tail verbatim — std's default arg
            // quoting would wrap the whole command line and break redirects.
            c.arg("/C").raw_arg(command);
            c
        };
        #[cfg(not(windows))]
        let mut cmd = {
            let mut c = std::process::Command::new("sh");
            c.args(["-c", command]);
            c
        };
        let spawned = cmd
            .env("REGENT_HOOK_EVENT", event)
            .env("REGENT_HOOK_TOOL", tool)
            .env("REGENT_HOOK_PAYLOAD", clipped)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        match spawned {
            // Reap off-thread so the child never zombies (Unix) and never
            // blocks the dispatch path.
            Ok(mut child) => {
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
            }
            Err(error) => tracing::warn!(%error, event, "lifecycle hook failed to spawn"),
        }
    }
}

impl DispatchHook for ShellHook {
    fn before_dispatch(&self, tool: &str, args: &Value) {
        Self::fire(
            self.on_start.as_deref(),
            "tool_start",
            tool,
            &args.to_string(),
        );
    }

    fn after_dispatch(&self, tool: &str, result: &str) {
        Self::fire(self.on_complete.as_deref(), "tool_complete", tool, result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_commands_deactivate_the_hook() {
        assert!(!ShellHook::new("", "  ").is_active());
        assert!(ShellHook::new("echo hi", "").is_active());
    }

    /// The hook actually runs the command with the event env vars — proven by
    /// a command that writes them to a file. Fire-and-forget, so poll briefly.
    #[test]
    fn fires_the_command_with_event_env() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("hook.txt");
        let command = if cfg!(windows) {
            format!(
                "echo %REGENT_HOOK_EVENT% %REGENT_HOOK_TOOL% > \"{}\"",
                marker.display()
            )
        } else {
            format!(
                "echo $REGENT_HOOK_EVENT $REGENT_HOOK_TOOL > '{}'",
                marker.display()
            )
        };
        let hook = ShellHook::new(&command, "");
        hook.before_dispatch("read_file", &serde_json::json!({"path": "x"}));

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !marker.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let content = std::fs::read_to_string(&marker).expect("hook wrote the marker");
        assert!(content.contains("tool_start"), "{content}");
        assert!(content.contains("read_file"), "{content}");
    }
}

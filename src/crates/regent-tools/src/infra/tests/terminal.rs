//! Unit tests for `terminal` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;
use crate::domain::contracts::{ApprovalDecision, ApprovalHandler, DenyAll};
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

#[test]
fn deacon_free_regent_subcommands_are_allowed_through() {
    // `gateway` never opens a deacon client (router.ts dispatches itdirectly),
    // so "set up Telegram for me" is work the agent can actually do.
    assert!(is_deacon_free_regent_command(
        "regent gateway setup 123:ABC"
    ));
    assert!(is_deacon_free_regent_command(
        "regent -p work gateway start"
    ));
    assert!(is_deacon_free_regent_command("regent GATEWAY status"));
    // Everything else still deadlocks a second deacon → stays blocked.
    assert!(!is_deacon_free_regent_command("regent status"));
    assert!(!is_deacon_free_regent_command("regent chat"));
    assert!(!is_deacon_free_regent_command("regent -p work model list"));
    // A safe segment cannot smuggle an unsafe one alongside it.
    assert!(!is_deacon_free_regent_command(
        "regent gateway status && regent chat"
    ));
}

#[tokio::test]
async fn regent_cli_command_is_short_circuited() {
    let out = TerminalTool::default()
        .execute(
            json!({"command": "regent status"}),
            &ctx_with(Arc::new(DenyAll)),
        )
        .await
        .unwrap();
    assert!(out.contains("running Regent deacon"), "got: {out}");
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

/// P0 regression (`64aad1f` → fixed 2026-07-27): the path jail became default-on
/// for every session, and this tool was reading `is_sandboxed()` to mean "the
/// input is untrusted". Result: no ordinary session could run a command — no
/// `npm install`, no build, no test — for a day. An ordinary local session is
/// path-jailed AND trusted, and it must get its shell.
#[tokio::test]
async fn a_path_jailed_but_trusted_session_still_gets_a_local_shell() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = ToolContext::new_sandboxed(
        dir.path().to_path_buf(),
        dir.path().to_path_buf(),
        Arc::new(DenyAll),
    );
    assert!(ctx.is_sandboxed(), "paths stay jailed — that rail is kept");
    assert!(!ctx.is_untrusted(), "but a local session is not untrusted");

    let out = TerminalTool::default()
        .execute(json!({"command": "echo regent-shell-restored"}), &ctx)
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["exit_code"], 0, "trusted session must execute: {v}");
    assert!(
        v["stdout"]
            .as_str()
            .unwrap()
            .contains("regent-shell-restored")
    );
}

// The jail must hold against the COMMAND string, and it can't — so an UNTRUSTED
// context gets no LOCAL shell. An isolated backend (docker/ssh) stays allowed:
// the container is that session's jail.
#[tokio::test]
async fn untrusted_context_gets_no_local_shell_but_keeps_isolated_backends() {
    struct Isolated;
    #[async_trait]
    impl crate::domain::contracts::TerminalBackend for Isolated {
        fn describe(&self) -> String {
            "docker:test".into()
        }
        async fn run(
            &self,
            _command: &str,
            _cwd: &std::path::Path,
            _timeout: std::time::Duration,
        ) -> Result<crate::domain::contracts::CommandOutput, RegentError> {
            Ok(crate::domain::contracts::CommandOutput {
                exit_code: Some(0),
                stdout: "ran-in-container".into(),
                stderr: String::new(),
            })
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let ctx = ToolContext::new_sandboxed(
        dir.path().to_path_buf(),
        dir.path().to_path_buf(),
        Arc::new(DenyAll),
    )
    .untrusted();

    // Local backend on untrusted input: refused, nothing executes.
    let out = TerminalTool::default()
        .execute(json!({"command": "echo should-not-run"}), &ctx)
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert!(
        v["error"]
            .as_str()
            .unwrap()
            .contains("externally-triggered"),
        "untrusted local terminal must refuse: {v}"
    );
    assert!(v.get("exit_code").is_none(), "nothing may execute");

    // Isolated backend in the same jail: allowed — the container is the jail.
    let out = TerminalTool::with_backend(Arc::new(Isolated))
        .execute(json!({"command": "echo hi"}), &ctx)
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["exit_code"], 0);
    assert_eq!(v["stdout"], "ran-in-container");
}

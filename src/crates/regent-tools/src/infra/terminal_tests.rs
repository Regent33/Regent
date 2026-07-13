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

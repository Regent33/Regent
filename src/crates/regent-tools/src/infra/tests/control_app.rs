//! Unit tests for `control_app` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;
use crate::domain::contracts::{ApprovalDecision, ApprovalHandler, DenyAll};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn ctx(approval: Arc<dyn ApprovalHandler>) -> ToolContext {
    ToolContext::new(std::env::temp_dir(), approval)
}

#[tokio::test]
async fn denied_without_approval_and_consults_the_gate() {
    struct Rec(AtomicBool);
    #[async_trait]
    impl ApprovalHandler for Rec {
        async fn request(&self, _: &str, _: &str, _: &str) -> ApprovalDecision {
            self.0.store(true, Ordering::SeqCst);
            ApprovalDecision::Deny
        }
    }
    let rec = Arc::new(Rec(AtomicBool::new(false)));
    let out = ControlAppTool
        .execute(
            json!({"script": "echo hi", "lang": "shell"}),
            &ctx(rec.clone()),
        )
        .await
        .unwrap();
    assert!(out.contains("denied by approval"));
    assert!(
        rec.0.load(Ordering::SeqCst),
        "approval gate must be consulted"
    );
}

#[tokio::test]
async fn missing_script_is_a_tool_error() {
    let out = ControlAppTool
        .execute(json!({}), &ctx(Arc::new(DenyAll)))
        .await
        .unwrap();
    assert!(out.contains("error"));
}

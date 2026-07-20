//! Approval-over-chat: /approve unblocks the gated tool; timeout denies.

use crate::{MockAdapter, allow, event, settle};
use async_trait::async_trait;
use regent_gateway::{
    ApprovalRouter, ChatApprovalHandler, ConversationHandler, GatewayRunner, RateLimiter,
};
use regent_kernel::RegentError;
use regent_tools::{ApprovalDecision, ApprovalHandler};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Handler that gates on chat approval (the dangerous-command path).
struct ApprovalGatedHandler {
    approval: Arc<ChatApprovalHandler>,
}

#[async_trait]
impl ConversationHandler for ApprovalGatedHandler {
    async fn handle(
        &self,
        _session_key: &str,
        _text: &str,
        _cancel: CancellationToken,
    ) -> Result<String, RegentError> {
        match self
            .approval
            .request("terminal", "rm -rf build/", "recursive deletion")
            .await
        {
            ApprovalDecision::Approve => Ok("ran the dangerous command".into()),
            // Gateway chat approvals are yes/no; feedback denials read the same.
            ApprovalDecision::Deny | ApprovalDecision::DenyWithFeedback(_) => {
                Ok("refused: not approved".into())
            }
        }
    }

    async fn reset(&self, _session_key: &str) {}
}

#[tokio::test]
async fn approval_over_chat_approve_and_timeout_deny() {
    let adapter = Arc::new(MockAdapter::default());
    let router = Arc::new(ApprovalRouter::new());
    let approval = Arc::new(ChatApprovalHandler::new(
        adapter.clone(),
        router.clone(),
        "mock:chat1",
        "chat1",
        Duration::from_millis(400),
    ));
    let runner = GatewayRunner::new(
        adapter.clone(),
        Arc::new(ApprovalGatedHandler { approval }),
        allow(&["alice"]),
        Arc::new(RateLimiter::per_minute(0)),
        router,
    );

    // Approve path: prompt arrives in chat, /approve unblocks the tool.
    runner.dispatch(event("alice", "clean the build dir")).await;
    settle().await;
    assert!(
        adapter
            .texts()
            .iter()
            .any(|t| t.contains("dangerous action"))
    );
    runner.dispatch(event("alice", "/approve")).await;
    settle().await;
    let texts = adapter.texts();
    assert!(texts.iter().any(|t| t.contains("Approved — continuing")));
    assert!(
        texts
            .iter()
            .any(|t| t.contains("ran the dangerous command"))
    );

    // Timeout path: nobody answers → deny by default.
    runner.dispatch(event("alice", "again")).await;
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert!(
        adapter
            .texts()
            .iter()
            .any(|t| t.contains("refused: not approved"))
    );

    // Stray /approve with nothing pending is a no-op answer.
    runner.dispatch(event("alice", "/approve")).await;
    assert!(
        adapter
            .texts()
            .last()
            .unwrap()
            .contains("No approval is pending")
    );
}

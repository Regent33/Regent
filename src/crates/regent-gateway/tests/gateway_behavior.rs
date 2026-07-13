//! M5 gateway contract via a mock adapter: auth + pairing, message
//! round-trip, /stop bypassing the busy guard, and approval-over-chat.

use async_trait::async_trait;
use regent_gateway::domain::auth::AuthSnapshot;
use regent_gateway::{
    ApprovalRouter, AuthPolicy, ChatApprovalHandler, ConversationHandler, GatewayError,
    GatewayRunner, MessageEvent, OutboundMessage, PlatformAdapter, RateLimiter,
};
use regent_kernel::RegentError;
use regent_tools::{ApprovalDecision, ApprovalHandler};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Test adapter: captures outbound messages; inbound is driven by calling
/// `dispatch` directly.
#[derive(Default)]
struct MockAdapter {
    sent: Mutex<Vec<OutboundMessage>>,
}

impl MockAdapter {
    fn texts(&self) -> Vec<String> {
        self.sent
            .lock()
            .unwrap()
            .iter()
            .map(|m| m.text.clone())
            .collect()
    }
}

#[async_trait]
impl PlatformAdapter for MockAdapter {
    fn platform(&self) -> &str {
        "mock"
    }

    async fn next_event(&self) -> Result<MessageEvent, GatewayError> {
        std::future::pending().await
    }

    async fn send(&self, message: OutboundMessage) -> Result<(), GatewayError> {
        self.sent.lock().unwrap().push(message);
        Ok(())
    }
}

struct EchoHandler;

#[async_trait]
impl ConversationHandler for EchoHandler {
    async fn handle(
        &self,
        _session_key: &str,
        text: &str,
        _cancel: CancellationToken,
    ) -> Result<String, RegentError> {
        Ok(format!("echo: {text}"))
    }

    async fn reset(&self, _session_key: &str) {}
}

/// Sleeps until cancelled — the long-running-turn stand-in.
struct SleepyHandler;

#[async_trait]
impl ConversationHandler for SleepyHandler {
    async fn handle(
        &self,
        _session_key: &str,
        _text: &str,
        cancel: CancellationToken,
    ) -> Result<String, RegentError> {
        tokio::select! {
            () = cancel.cancelled() => Err(RegentError::Interrupted),
            () = tokio::time::sleep(Duration::from_secs(30)) => Ok("finished".into()),
        }
    }

    async fn reset(&self, _session_key: &str) {}
}

fn event(user: &str, text: &str) -> MessageEvent {
    MessageEvent {
        platform: "mock".into(),
        chat_id: "chat1".into(),
        user_id: user.into(),
        text: text.into(),
    }
}

fn allow(users: &[&str]) -> Arc<AuthPolicy> {
    Arc::new(AuthPolicy::new(AuthSnapshot {
        allow_all: false,
        allowlist: users.iter().map(|u| format!("mock:{u}")).collect(),
        paired: Default::default(),
    }))
}

async fn settle() {
    tokio::time::sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn round_trip_with_auth_and_pairing() {
    let adapter = Arc::new(MockAdapter::default());
    let runner = GatewayRunner::new(
        adapter.clone(),
        Arc::new(EchoHandler),
        allow(&["alice"]),
        Arc::new(RateLimiter::per_minute(0)),
        Arc::new(ApprovalRouter::new()),
    );

    // Authorized round-trip.
    runner.dispatch(event("alice", "hello there")).await;
    settle().await;
    assert!(adapter.texts().contains(&"echo: hello there".to_owned()));

    // Stranger denied; alice issues a code; stranger redeems → authorized.
    runner.dispatch(event("bob", "let me in")).await;
    assert!(adapter.texts().last().unwrap().contains("Not authorized"));
    runner.dispatch(event("alice", "/pair")).await;
    let code = adapter
        .texts()
        .last()
        .unwrap()
        .split_whitespace()
        .last()
        .unwrap()
        .to_owned();
    runner.dispatch(event("bob", &code)).await;
    assert!(adapter.texts().last().unwrap().contains("Paired"));
    runner.dispatch(event("bob", "hi from bob")).await;
    settle().await;
    assert!(adapter.texts().contains(&"echo: hi from bob".to_owned()));
}

#[tokio::test]
async fn stop_bypasses_the_busy_guard_and_cancels_the_turn() {
    let adapter = Arc::new(MockAdapter::default());
    let runner = GatewayRunner::new(
        adapter.clone(),
        Arc::new(SleepyHandler),
        allow(&["alice"]),
        Arc::new(RateLimiter::per_minute(0)),
        Arc::new(ApprovalRouter::new()),
    );

    runner.dispatch(event("alice", "do something slow")).await;
    settle().await;
    // Plain messages are guarded while the turn runs…
    runner.dispatch(event("alice", "are you done?")).await;
    assert!(adapter.texts().last().unwrap().contains("Still working"));
    // …but /stop reaches the runner and cancels the in-flight turn.
    runner.dispatch(event("alice", "/stop")).await;
    settle().await;
    let texts = adapter.texts();
    assert!(texts.iter().any(|t| t.contains("Stopping")));
    assert!(
        texts.iter().any(|t| t.contains("interrupted")),
        "turn must end interrupted"
    );

    // Guard released — next message runs again.
    runner.dispatch(event("alice", "/stop")).await;
    assert!(
        adapter
            .texts()
            .last()
            .unwrap()
            .contains("Nothing is running")
    );
}

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

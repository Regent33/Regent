//! M5 gateway contract via a mock adapter: auth + pairing, message
//! round-trip, /stop bypassing the busy guard, and approval-over-chat.
//! The mock adapter/handlers live here; tests are in the submodules.

use async_trait::async_trait;
use regent_gateway::domain::auth::AuthSnapshot;
use regent_gateway::{
    AuthPolicy, ConversationHandler, GatewayError, MessageEvent, OutboundMessage, PlatformAdapter,
};
use regent_kernel::RegentError;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

mod approvals;
mod messaging;

/// Test adapter: captures outbound messages; inbound is driven by calling
/// `dispatch` directly.
#[derive(Default)]
pub struct MockAdapter {
    sent: Mutex<Vec<OutboundMessage>>,
}

impl MockAdapter {
    pub fn texts(&self) -> Vec<String> {
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

pub struct EchoHandler;

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
pub struct SleepyHandler;

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

pub fn event(user: &str, text: &str) -> MessageEvent {
    MessageEvent {
        platform: "mock".into(),
        chat_id: "chat1".into(),
        user_id: user.into(),
        text: text.into(),
    }
}

pub fn allow(users: &[&str]) -> Arc<AuthPolicy> {
    Arc::new(AuthPolicy::new(AuthSnapshot {
        allow_all: false,
        allowlist: users.iter().map(|u| format!("mock:{u}")).collect(),
        paired: Default::default(),
    }))
}

/// Point `REGENT_HOME` at an empty directory for this test binary.
///
/// `ChatApprovalHandler::request` consults `auto_approve_enabled()`, which reads
/// `$REGENT_HOME/config.yaml` and the `gateway-auto` flag file — i.e. the
/// DEVELOPER'S REAL machine config. On a machine with `tools.auto_approve: true`
/// (a normal thing for an owner to set) every approval is granted before a
/// prompt is ever sent, so the approval tests failed locally while passing in
/// CI. Worse than the noise: a security test that a personal setting can switch
/// off cannot be trusted to catch a regression in the approval path.
///
/// `LazyLock` so the env is set exactly once per process, before any test body
/// runs, and the TempDir is leaked deliberately — it must outlive every test.
pub fn isolate_regent_home() {
    static HOME: std::sync::LazyLock<()> = std::sync::LazyLock::new(|| {
        let dir = tempfile::tempdir().expect("temp REGENT_HOME");
        // SAFETY: runs once, inside LazyLock's initialization lock, before the
        // tests that read REGENT_HOME do any work.
        unsafe { std::env::set_var("REGENT_HOME", dir.path()) };
        std::mem::forget(dir);
    });
    *HOME
}

pub async fn settle() {
    tokio::time::sleep(Duration::from_millis(100)).await;
}

/// Finishes the work but returns no final text — the "job done, dead silence"
/// shape (model stopped on a tool call, or hit the iteration cap).
pub struct SilentHandler;

#[async_trait]
impl ConversationHandler for SilentHandler {
    async fn handle(
        &self,
        _session_key: &str,
        _text: &str,
        _cancel: CancellationToken,
    ) -> Result<String, RegentError> {
        Ok("   \n".to_owned())
    }

    async fn reset(&self, _session_key: &str) {}
}

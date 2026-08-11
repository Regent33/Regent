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

/// Neutralise every switch that can auto-approve, so the approval tests test
/// the approval path.
///
/// `auto_approve_enabled()` (approval.rs:134) has THREE of them, checked in
/// order: the `REGENT_AUTO_APPROVE` env var, the `gateway-auto` flag file, and
/// `tools.auto_approve` in `$REGENT_HOME/config.yaml` — i.e. the DEVELOPER'S
/// REAL machine config. On a machine with `auto_approve: true` (a normal thing
/// for an owner to set) every approval is granted before a prompt is ever sent,
/// so these tests failed locally while passing in CI. Worse than the noise: a
/// security test that a personal setting can switch off sits green on exactly
/// the machine most likely to have that setting, and cannot catch a regression
/// in the approval path there.
///
/// Pointing `REGENT_HOME` at an empty dir covers the last two, because both
/// live under it. The env var has to be cleared separately — isolating two of
/// three leaves the same hole, just narrower. The sibling unit test at
/// approval.rs:243 pairs exactly these two calls.
///
/// `LazyLock` so this happens once per test binary however many tests call it,
/// and under an initialization lock so two tests racing here cannot interleave
/// the two `set_var`/`remove_var` calls. Each caller invokes it as its first
/// statement.
pub fn isolate_regent_home() {
    static HOME: std::sync::LazyLock<()> = std::sync::LazyLock::new(|| {
        // `keep()` rather than letting it drop: the directory must outlive every
        // test in the binary, and it says so, where `mem::forget` only implies
        // it. It does leave an empty dir under TEMP per run.
        let dir = tempfile::tempdir().expect("temp REGENT_HOME").keep();
        // SAFETY: inside LazyLock's initialization lock, and every reader of
        // these variables in this binary goes through this function first.
        unsafe {
            std::env::set_var("REGENT_HOME", &dir);
            std::env::remove_var("REGENT_AUTO_APPROVE");
        }
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

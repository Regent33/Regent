//! Approval-over-chat: a dangerous tool action sends a prompt to the chat
//! and blocks until `/approve` or `/deny` arrives (routed by the runner) —
//! or times out. Non-response is a **deny**, never proceed-by-default
//! (a core invariant).

use crate::domain::contracts::PlatformAdapter;
use crate::domain::entities::OutboundMessage;
use async_trait::async_trait;
use regent_tools::{ApprovalDecision, ApprovalHandler};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::oneshot;

/// Routes chat replies to whichever approval is pending in that chat.
#[derive(Default)]
pub struct ApprovalRouter {
    pending: Mutex<HashMap<String, oneshot::Sender<bool>>>,
}

impl ApprovalRouter {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a pending approval for `chat_key`. A previous pending one
    /// in the same chat is dropped (its waiter resolves to deny).
    pub fn register(&self, chat_key: &str) -> oneshot::Receiver<bool> {
        let (sender, receiver) = oneshot::channel();
        self.pending
            .lock()
            .expect("approval mutex poisoned")
            .insert(chat_key.to_owned(), sender);
        receiver
    }

    /// `/approve` / `/deny` arrived. Returns false when nothing was pending.
    pub fn resolve(&self, chat_key: &str, approved: bool) -> bool {
        match self
            .pending
            .lock()
            .expect("approval mutex poisoned")
            .remove(chat_key)
        {
            Some(sender) => sender.send(approved).is_ok(),
            None => false,
        }
    }
}

/// `regent_tools::ApprovalHandler` bound to one chat: the gateway's answer
/// to the CLI's stdin y/N prompt.
pub struct ChatApprovalHandler {
    adapter: Arc<dyn PlatformAdapter>,
    router: Arc<ApprovalRouter>,
    chat_key: String,
    chat_id: String,
    timeout: Duration,
}

impl ChatApprovalHandler {
    #[must_use]
    pub fn new(
        adapter: Arc<dyn PlatformAdapter>,
        router: Arc<ApprovalRouter>,
        chat_key: impl Into<String>,
        chat_id: impl Into<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            adapter,
            router,
            chat_key: chat_key.into(),
            chat_id: chat_id.into(),
            timeout,
        }
    }
}

/// Auto mode (`tools.auto_approve` in config.yaml, or `REGENT_AUTO_APPROVE=1`).
///
/// Read from disk **per request** rather than cached at startup, deliberately:
/// the gateway is a long-lived background process, and a user who switches auto
/// mode OFF in the app must have that take effect without remembering to
/// restart it. One small file read per approval gate is nothing next to the
/// model call that triggered it.
fn auto_approve_enabled() -> bool {
    if std::env::var("REGENT_AUTO_APPROVE").is_ok_and(|v| matches!(v.trim(), "1" | "true" | "yes"))
    {
        return true;
    }
    // Same `$REGENT_HOME` else `~/.regent` resolution as every other surface.
    let home = std::env::var("REGENT_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            let base = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_default();
            std::path::PathBuf::from(base).join(".regent")
        });
    let Ok(raw) = std::fs::read_to_string(home.join("config.yaml")) else {
        return false;
    };
    serde_yaml::from_str::<serde_yaml::Value>(&raw)
        .ok()
        .and_then(|cfg| cfg.get("tools")?.get("auto_approve")?.as_bool())
        .unwrap_or(false)
}

#[async_trait]
impl ApprovalHandler for ChatApprovalHandler {
    async fn request(&self, tool: &str, action: &str, reason: &str) -> ApprovalDecision {
        if auto_approve_enabled() {
            tracing::info!(tool, reason, "auto-approved (tools.auto_approve)");
            return ApprovalDecision::Approve;
        }
        let receiver = self.router.register(&self.chat_key);
        let prompt = OutboundMessage {
            chat_id: self.chat_id.clone(),
            text: format!(
                "⚠ {tool} wants to run a dangerous action ({reason}):\n{action}\n\nReply /approve or /deny — denying in {}s otherwise.",
                self.timeout.as_secs()
            ),
        };
        if let Err(error) = self.adapter.send(prompt).await {
            tracing::warn!(%error, "could not deliver approval prompt; denying");
            return ApprovalDecision::Deny;
        }
        match tokio::time::timeout(self.timeout, receiver).await {
            Ok(Ok(true)) => ApprovalDecision::Approve,
            // timeout, dropped sender, or explicit deny — all deny.
            _ => ApprovalDecision::Deny,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Auto mode is read per request, not cached: switching it OFF in the app
    /// has to reach a gateway that has been running for days. The env var is
    /// process-global, so this test owns the config-file path only.
    #[test]
    fn auto_approve_follows_the_config_file_between_calls() {
        let dir = tempfile::tempdir().expect("tempdir");
        // SAFETY: single-threaded test; no other thread reads REGENT_HOME here.
        unsafe {
            std::env::set_var("REGENT_HOME", dir.path());
            std::env::remove_var("REGENT_AUTO_APPROVE");
        }

        // No config at all → never auto-approve.
        assert!(!auto_approve_enabled(), "must fail closed without config");

        let config = dir.path().join("config.yaml");
        std::fs::write(&config, "tools:\n  auto_approve: true\n").unwrap();
        assert!(auto_approve_enabled(), "on when the file says on");

        std::fs::write(&config, "tools:\n  auto_approve: false\n").unwrap();
        assert!(!auto_approve_enabled(), "OFF must apply without a restart");

        unsafe { std::env::remove_var("REGENT_HOME") };
    }
}

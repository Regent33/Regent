//! Approval-over-chat: a dangerous tool action sends a prompt to the chat
//! and blocks until `/approve` or `/deny` arrives (routed by the runner) —
//! or times out. Non-response is a **deny**, never proceed-by-default
//! (Hermes invariant).

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
        match self.pending.lock().expect("approval mutex poisoned").remove(chat_key) {
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

#[async_trait]
impl ApprovalHandler for ChatApprovalHandler {
    async fn request(&self, tool: &str, action: &str, reason: &str) -> ApprovalDecision {
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

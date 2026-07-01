//! Per-session plumbing: the RPC approval handler, the tool-event hook, and
//! the session-registry entry. Constructed by the manager's lifecycle code.

use crate::domain::contracts::OutboundTx;
use crate::domain::entities::RpcNotification;
use async_trait::async_trait;
use regent_agent::Agent;
use regent_kernel::RegentError;
use regent_tools::{ApprovalDecision, ApprovalHandler, DeliverySink, DispatchHook};
use serde_json::json;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::{Mutex, oneshot};
use tokio_util::sync::CancellationToken;

const APPROVAL_TIMEOUT: Duration = Duration::from_secs(120);

/// Bridges a tool approval request to the client over JSON-RPC: emits
/// `approval.request`, then blocks on a oneshot resolved by `approval.respond`
/// (deny on timeout — never proceed without an explicit yes).
pub(super) struct RpcApprovalHandler {
    /// Filled after `Agent::new` returns (the agent generates the id).
    pub(super) session_id: Arc<OnceLock<String>>,
    pub(super) out_tx: OutboundTx,
    pub(super) pending: Arc<Mutex<Option<oneshot::Sender<bool>>>>,
}

#[async_trait]
impl ApprovalHandler for RpcApprovalHandler {
    async fn request(&self, tool: &str, action: &str, reason: &str) -> ApprovalDecision {
        let (tx, rx) = oneshot::channel();
        *self.pending.lock().await = Some(tx);

        let sid = self.session_id.get().cloned().unwrap_or_default();
        let notif = RpcNotification::new(
            "approval.request",
            json!({ "session_id": sid, "tool": tool, "action": action, "reason": reason }),
        );
        if let Ok(line) = serde_json::to_string(&notif) {
            self.out_tx.send(line).ok();
        }

        match tokio::time::timeout(APPROVAL_TIMEOUT, rx).await {
            Ok(Ok(true)) => ApprovalDecision::Approve,
            _ => {
                *self.pending.lock().await = None;
                ApprovalDecision::Deny
            }
        }
    }
}

/// Emits `tool.start` / `tool.complete` around every dispatch (the ADR-011
/// event surface; the CLI renders these as activity lines).
pub(super) struct RpcToolHook {
    pub(super) session_id: Arc<OnceLock<String>>,
    pub(super) out_tx: OutboundTx,
}

impl RpcToolHook {
    fn emit(&self, method: &str, params: serde_json::Value) {
        if let Ok(line) = serde_json::to_string(&RpcNotification::new(method, params)) {
            self.out_tx.send(line).ok();
        }
    }
}

impl DispatchHook for RpcToolHook {
    fn before_dispatch(&self, tool: &str, _args: &serde_json::Value) {
        let sid = self.session_id.get().cloned().unwrap_or_default();
        self.emit("tool.start", json!({ "session_id": sid, "tool": tool }));
    }

    fn after_dispatch(&self, tool: &str, result: &str) {
        let sid = self.session_id.get().cloned().unwrap_or_default();
        let is_error = serde_json::from_str::<serde_json::Value>(result)
            .is_ok_and(|v| v.get("error").is_some());
        self.emit(
            "tool.complete",
            json!({ "session_id": sid, "tool": tool, "is_error": is_error }),
        );
    }
}

/// One live session in the registry.
pub(super) struct SessionEntry {
    pub(super) agent: Arc<Mutex<Agent>>,
    /// Cancel the currently running turn; None when no turn is active.
    pub(super) interrupt: Arc<Mutex<Option<CancellationToken>>>,
    /// Oneshot sender to resolve a pending `approval.request`.
    pub(super) approval_pending: Arc<Mutex<Option<oneshot::Sender<bool>>>>,
}

/// Deacon-native delivery sink for `send_message`: the connected surface *is*
/// the channel, so a delivery becomes a `message.outbound` notification the CLI
/// renders. (Real platform sinks — Telegram/Discord — plug in at the gateway.)
pub(super) struct NotificationDelivery {
    pub(super) session_id: Arc<OnceLock<String>>,
    pub(super) out_tx: OutboundTx,
}

#[async_trait]
impl DeliverySink for NotificationDelivery {
    async fn deliver(&self, target: &str, text: &str) -> Result<(), RegentError> {
        let sid = self.session_id.get().cloned().unwrap_or_default();
        let to = if target.is_empty() { "home" } else { target };
        let notif = RpcNotification::new(
            "message.outbound",
            json!({ "session_id": sid, "target": to, "text": text }),
        );
        if let Ok(line) = serde_json::to_string(&notif) {
            self.out_tx.send(line).ok();
        }
        Ok(())
    }

    fn targets(&self) -> Vec<String> {
        vec!["home".to_owned()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::unbounded_channel;

    #[tokio::test]
    async fn delivery_emits_a_message_outbound_notification() {
        let (tx, mut rx) = unbounded_channel();
        let cell: Arc<OnceLock<String>> = Arc::new(OnceLock::new());
        let _ = cell.set("sess_1".to_owned());
        let sink = NotificationDelivery {
            session_id: cell,
            out_tx: tx,
        };

        sink.deliver("", "build is green").await.unwrap();
        let line = rx.recv().await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["method"], "message.outbound");
        assert_eq!(v["params"]["target"], "home");
        assert_eq!(v["params"]["text"], "build is green");
        assert_eq!(v["params"]["session_id"], "sess_1");
    }
}

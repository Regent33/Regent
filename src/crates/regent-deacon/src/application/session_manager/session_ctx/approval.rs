//! Approval posture selection for one session.

use super::{SessionManager, env_flag};
use crate::application::session_manager::hooks::{ApprovalTx, RpcApprovalHandler};
use regent_tools::{ApprovalDecision, ApprovalHandler};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;

/// Config auto mode is checked per request so a live toggle affects open
/// sessions. `ask_user` still reaches the human: auto means skip permission
/// prompts, not answer the agent's questions with a blanket yes.
pub(super) struct ConfigGatedApprover {
    pub(super) auto: Arc<AtomicBool>,
    pub(super) inner: RpcApprovalHandler,
}

#[async_trait::async_trait]
impl ApprovalHandler for ConfigGatedApprover {
    async fn request(&self, tool: &str, action: &str, reason: &str) -> ApprovalDecision {
        if tool != "ask_user" && self.auto.load(Ordering::Acquire) {
            return ApprovalDecision::Approve;
        }
        self.inner.request(tool, action, reason).await
    }
}

/// Env-fixed posture, or `None` when the RPC/config path should be used.
pub(super) fn env_auto_approver() -> Option<Arc<dyn ApprovalHandler>> {
    if !env_flag("REGENT_AUTO_APPROVE") {
        return None;
    }
    if env_flag("REGENT_VOICE") && !env_flag("REGENT_VOICE_FULL_CONTROL") {
        Some(Arc::new(regent_tools::VoiceScopedApprover))
    } else {
        Some(Arc::new(regent_tools::AllowAll))
    }
}

impl SessionManager {
    /// Voice defaults to the deny-all named posture; full-control and non-voice
    /// auto sessions opt into blanket approval. Otherwise prompt over RPC.
    pub(in crate::application::session_manager) fn approval_handler(
        &self,
        sid_cell: &Arc<OnceLock<String>>,
        approval_pending: &Arc<Mutex<Option<ApprovalTx>>>,
    ) -> Arc<dyn ApprovalHandler> {
        if let Some(handler) = env_auto_approver() {
            return handler;
        }
        Arc::new(ConfigGatedApprover {
            auto: Arc::clone(&self.auto_approve),
            inner: RpcApprovalHandler {
                session_id: Arc::clone(sid_cell),
                out_tx: self.out_tx.clone(),
                pending: Arc::clone(approval_pending),
            },
        })
    }
}

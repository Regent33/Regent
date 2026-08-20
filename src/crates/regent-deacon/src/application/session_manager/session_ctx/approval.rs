//! Approval posture selection for one session.

use super::{SessionManager, env_flag};
use crate::application::session_manager::hooks::{ApprovalTx, RpcApprovalHandler};
use regent_kernel::contracts::questionnaire::{Questionnaire, QuestionnaireAnswer};
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

    /// A question is never a permission prompt, so auto mode never answers it.
    async fn request_structured(&self, questionnaire: &Questionnaire) -> QuestionnaireAnswer {
        self.inner.request_structured(questionnaire).await
    }
}

/// Env-fixed posture for tool GATES, or `None` when the RPC/config path should
/// be used. Deliberately not used for questions — see [`EnvGatedApprover`].
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

/// An env-fixed posture that still lets the agent ASK. `AllowAll` used to
/// answer every `ask_user` question with a blanket "yes" — the one gap
/// `ConfigGatedApprover` had always closed — so an auto session silently
/// agreed to whatever the model proposed.
///
/// The two auto surfaces need different treatment, because they differ in
/// whether a human can actually see a prompt:
///
/// - **A non-voice auto session** has a client on the other end of the RPC
///   channel. Questions route there like any other session.
/// - **A voice call** has no card to draw and no keyboard. Stalling for the
///   full 120s timeout would be dead air, so a question resolves immediately
///   as unanswered; the model proceeds on its judgment and states the
///   assumption — or simply asks out loud, which is the native move on a call.
pub(super) struct EnvGatedApprover {
    pub(super) gates: Arc<dyn ApprovalHandler>,
    pub(super) questions: Option<RpcApprovalHandler>,
}

#[async_trait::async_trait]
impl ApprovalHandler for EnvGatedApprover {
    async fn request(&self, tool: &str, action: &str, reason: &str) -> ApprovalDecision {
        match (&self.questions, tool) {
            (Some(rpc), "ask_user") => rpc.request(tool, action, reason).await,
            _ => self.gates.request(tool, action, reason).await,
        }
    }

    async fn request_structured(&self, questionnaire: &Questionnaire) -> QuestionnaireAnswer {
        match &self.questions {
            Some(rpc) => rpc.request_structured(questionnaire).await,
            None => QuestionnaireAnswer {
                questionnaire_id: questionnaire.id.clone(),
                answers: Vec::new(),
                cancelled: true,
            },
        }
    }
}

impl SessionManager {
    /// Voice defaults to the deny-all named posture; full-control and non-voice
    /// auto sessions opt into blanket approval. Otherwise prompt over RPC.
    /// Every posture routes QUESTIONS to the human when the surface has one.
    pub(in crate::application::session_manager) fn approval_handler(
        &self,
        sid_cell: &Arc<OnceLock<String>>,
        approval_pending: &Arc<Mutex<Option<ApprovalTx>>>,
    ) -> Arc<dyn ApprovalHandler> {
        let rpc = || RpcApprovalHandler {
            session_id: Arc::clone(sid_cell),
            out_tx: self.out_tx.clone(),
            pending: Arc::clone(approval_pending),
            supports_questions: Arc::clone(&self.client_supports_questions),
        };
        if let Some(gates) = env_auto_approver() {
            return Arc::new(EnvGatedApprover {
                gates,
                // A hands-free call has no surface to render a question on.
                questions: (!env_flag("REGENT_VOICE")).then(rpc),
            });
        }
        Arc::new(ConfigGatedApprover {
            auto: Arc::clone(&self.auto_approve),
            inner: rpc(),
        })
    }
}

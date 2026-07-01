//! Session create/resume: build (or replay) an Agent, wire its approval handler,
//! delta sink, and review fork, then register it. `create_session_keyed`'s
//! `plan_mode` flag strips the catalog to the read-only subset for the
//! `code.plan` phase, so that turn physically cannot edit.

use super::SessionManager;
use super::hooks::RpcApprovalHandler;
use crate::domain::errors::DeaconError;
use regent_agent::Agent;
use regent_kernel::SessionId;
use regent_tools::ToolContext;
use std::sync::{Arc, OnceLock};
use tokio::sync::{Mutex, oneshot};

impl SessionManager {
    pub async fn create_session(&self) -> Result<SessionId, DeaconError> {
        self.create_session_keyed(None, false).await
    }

    /// Approval handler for a new session. A surface with no way to prompt (a live
    /// voice call) sets `REGENT_AUTO_APPROVE=1` to approve automatically — opt-in,
    /// per dedicated daemon; otherwise approvals route to the client over RPC.
    fn approval_handler(
        &self,
        sid_cell: &Arc<OnceLock<String>>,
        approval_pending: &Arc<Mutex<Option<oneshot::Sender<bool>>>>,
    ) -> Arc<dyn regent_tools::ApprovalHandler> {
        let auto = std::env::var("REGENT_AUTO_APPROVE")
            .map(|v| matches!(v.trim(), "1" | "true" | "TRUE" | "yes"))
            .unwrap_or(false);
        if auto {
            Arc::new(regent_tools::AllowAll)
        } else {
            Arc::new(RpcApprovalHandler {
                session_id: Arc::clone(sid_cell),
                out_tx: self.out_tx.clone(),
                pending: Arc::clone(approval_pending),
            })
        }
    }

    pub(super) async fn create_session_keyed(
        &self,
        key: Option<&str>,
        plan_mode: bool,
    ) -> Result<SessionId, DeaconError> {
        let sid_cell: Arc<OnceLock<String>> = Arc::new(OnceLock::new());
        let approval_pending: Arc<Mutex<Option<oneshot::Sender<bool>>>> =
            Arc::new(Mutex::new(None));
        let approval = self.approval_handler(&sid_cell, &approval_pending);
        let provider = self.provider();
        let (mut catalog, review_catalog, system_prompt) = self
            .make_catalogs_and_prompt(&provider, &sid_cell, key)
            .await?;
        // Plan-mode (the `code.plan` read-only phase): restrict to the read-only
        // subset so the plan turn physically cannot edit — write/terminal tools
        // are absent from its catalog, not merely discouraged by the prompt.
        if plan_mode {
            let names: Vec<String> = catalog.definitions().into_iter().map(|d| d.name).collect();
            catalog.restrict_to(&regent_code::plan_toolset(regent_code::Phase::Plan, &names));
        }
        let ctx = if regent_tools::sandbox_enabled() {
            ToolContext::new_sandboxed(self.cwd.clone(), self.cwd.clone(), approval)
        } else {
            ToolContext::new(self.cwd.clone(), approval)
        };
        let agent = Agent::new(
            Arc::clone(&provider),
            Arc::new(catalog),
            Arc::clone(&self.store),
            ctx,
            system_prompt,
            self.agent_config(),
        )
        .map_err(DeaconError::Core)?
        .with_graph_memory(Arc::clone(&self.graph))
        .with_background_review(Self::review_setup(review_catalog))
        .with_delta_sink(self.delta_sink(&sid_cell));

        let id = agent.session_id().clone();
        let _ = sid_cell.set(id.to_string());
        self.entries
            .lock()
            .await
            .insert(id.clone(), self.make_entry(agent, approval_pending));
        Ok(id)
    }

    pub async fn resume_session(&self, session_id: SessionId) -> Result<SessionId, DeaconError> {
        self.resume_session_keyed(session_id, None).await
    }

    pub(super) async fn resume_session_keyed(
        &self,
        session_id: SessionId,
        key: Option<&str>,
    ) -> Result<SessionId, DeaconError> {
        self.store
            .session_meta(&session_id)
            .map_err(DeaconError::Store)?;

        let sid_cell: Arc<OnceLock<String>> = Arc::new(OnceLock::new());
        let _ = sid_cell.set(session_id.to_string());
        let approval_pending: Arc<Mutex<Option<oneshot::Sender<bool>>>> =
            Arc::new(Mutex::new(None));
        let approval = self.approval_handler(&sid_cell, &approval_pending);
        let provider = self.provider();
        let (catalog, review_catalog, system_prompt) = self
            .make_catalogs_and_prompt(&provider, &sid_cell, key)
            .await?;
        let ctx = if regent_tools::sandbox_enabled() {
            ToolContext::new_sandboxed(self.cwd.clone(), self.cwd.clone(), approval)
        } else {
            ToolContext::new(self.cwd.clone(), approval)
        };
        let agent = Agent::resume(
            Arc::clone(&provider),
            Arc::new(catalog),
            Arc::clone(&self.store),
            ctx,
            system_prompt,
            self.agent_config(),
            session_id.clone(),
        )
        .map_err(DeaconError::Core)?
        .with_graph_memory(Arc::clone(&self.graph))
        .with_background_review(Self::review_setup(review_catalog))
        .with_delta_sink(self.delta_sink(&sid_cell));

        self.entries
            .lock()
            .await
            .insert(session_id.clone(), self.make_entry(agent, approval_pending));
        Ok(session_id)
    }

    /// The session bound to `conversation_key`, creating + binding a fresh one
    /// when there's no binding (or the bound session is gone). Gives platform
    /// surfaces per-conversation continuity across messages.
    pub async fn ensure_keyed_session(
        &self,
        conversation_key: &str,
    ) -> Result<SessionId, DeaconError> {
        if let Some(stored) = self.store.conversation_session(conversation_key)? {
            let sid = SessionId::from_string(&stored);
            // Already live in memory → reuse it.
            if self.entries.lock().await.contains_key(&sid) {
                return Ok(sid);
            }
            // Bound but cold → resume it (also validates it still exists).
            if let Ok(resumed) = self.resume_session_keyed(sid, Some(conversation_key)).await {
                return Ok(resumed);
            }
            // Stale binding (session purged) → fall through and recreate.
        }
        let sid = self
            .create_session_keyed(Some(conversation_key), false)
            .await?;
        self.store
            .bind_conversation(conversation_key, &sid.to_string())?;
        Ok(sid)
    }
}

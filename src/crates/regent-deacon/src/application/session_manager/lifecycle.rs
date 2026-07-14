//! Session create/resume: build (or replay) an Agent, wire its approval handler,
//! delta sink, and review fork, then register it. `create_session_keyed`'s
//! `plan_mode` flag strips the catalog to the read-only subset for the
//! `code.plan` phase, so that turn physically cannot edit.

use super::SessionManager;
use super::hooks::ApprovalTx;
use crate::domain::errors::DeaconError;
use regent_agent::Agent;
use regent_kernel::SessionId;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;

/// What a session is being born for. `CodePlan` restricts the catalog to the
/// read-only subset (the turn physically cannot edit); `CodeExecute` wraps the
/// editing tools with edit-time diagnostics; `Chat` is everything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SessionKind {
    Chat,
    CodePlan,
    CodeExecute,
}

impl SessionManager {
    pub async fn create_session(&self) -> Result<SessionId, DeaconError> {
        self.create_session_keyed(None, SessionKind::Chat, None)
            .await
    }

    /// One autonomous turn on a fresh full-toolset session — the
    /// `background_task` tool's detached job. The caller isn't waiting on it;
    /// clients ignore its streamed deltas via their session-id filters.
    pub async fn run_detached_task(&self, task: &str) -> Result<String, DeaconError> {
        let session_id = self
            .create_session_keyed(None, SessionKind::Chat, None)
            .await?;
        self.run_turn(
            &session_id,
            &format!(
                "[Background job — no user is present to answer questions; work autonomously to \
                 completion and end with a concise report of what you produced and where it \
                 lives.]\n\n{task}"
            ),
        )
        .await
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
        let approval_pending: Arc<Mutex<Option<ApprovalTx>>> = Arc::new(Mutex::new(None));
        let approval = self.approval_handler(&sid_cell, &approval_pending);
        let provider = self.provider();
        let (catalog, review_catalog, mut ledger) = self
            .make_catalogs_and_prompt(&provider, &sid_cell, key, None)
            .await?;
        ledger.seal(&serde_json::to_string(&catalog.definitions()).unwrap_or_default());
        let system_prompt = ledger.render();
        let ctx = self.tool_context(key.is_some(), approval);
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

        // Resume keeps the STORED prompt when it differs from a fresh render;
        // rebase the baseline onto the bytes the agent will actually send so a
        // legitimately different stored prompt never reads as a cache bust.
        ledger.rebase(agent.system_prompt());
        self.entries.lock().await.insert(
            session_id.clone(),
            self.make_entry(agent, approval_pending, ledger),
        );
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
            .create_session_keyed(Some(conversation_key), SessionKind::Chat, None)
            .await?;
        self.store
            .bind_conversation(conversation_key, &sid.to_string())?;
        Ok(sid)
    }
}

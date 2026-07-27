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
    /// A detached autonomous job (`background_task`): chat-shaped catalog,
    /// but agentic by nature — never routed to the `light` profile (ADR-038
    /// P1), and no `ask_user` (nobody is present to answer).
    Background,
}

impl SessionManager {
    pub async fn create_session(&self) -> Result<SessionId, DeaconError> {
        self.create_session_with_workspace(None).await
    }

    /// Desktop's per-session workspace entry point: the session's tools run in
    /// `workspace` (jailed to it) instead of the deacon's own cwd. `None` is
    /// the historical behavior every other caller keeps.
    pub async fn create_session_with_workspace(
        &self,
        workspace: Option<std::path::PathBuf>,
    ) -> Result<SessionId, DeaconError> {
        self.create_session_keyed(None, SessionKind::Chat, None, workspace)
            .await
    }

    /// One autonomous turn on a fresh full-toolset session — the
    /// `background_task` tool's detached job. The caller isn't waiting on it;
    /// clients ignore its streamed deltas via their session-id filters.
    /// `on_session` is called with the detached session's id as soon as it
    /// exists, before the turn runs. The job ledger uses it to record where the
    /// work is happening — that transcript is the evidence behind any later
    /// claim about the job, and without it a job row points at nothing.
    pub async fn run_detached_task(
        &self,
        task: &str,
        on_session: impl FnOnce(&SessionId),
    ) -> Result<String, DeaconError> {
        let session_id = self
            .create_session_keyed(None, SessionKind::Background, None, None)
            .await?;
        on_session(&session_id);
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
        // A live entry already owns the authoritative transcript and its
        // per-session turn mutex. Rebuilding here would replace that mutex and
        // let a resumed UI run a second turn concurrently against stale history.
        if self.entries.lock().await.contains_key(&session_id) {
            return Ok(session_id);
        }

        self.store
            .session_meta(&session_id)
            .map_err(DeaconError::Store)?;

        let sid_cell: Arc<OnceLock<String>> = Arc::new(OnceLock::new());
        let _ = sid_cell.set(session_id.to_string());
        let approval_pending: Arc<Mutex<Option<ApprovalTx>>> = Arc::new(Mutex::new(None));
        let approval = self.approval_handler(&sid_cell, &approval_pending);
        let provider = self.provider();
        // Resume on the profile the session actually runs (ADR-038 P1): a
        // light-born session that never escalated keeps its light catalog —
        // its stored prompt owns a light-defs cache, and a full rebuild here
        // would bust it AND leave the session counted light-unescalated in
        // the telemetry while really running full. Escalated, pre-P1, and
        // kill-switched sessions resume full as before.
        let (profile, escalated) = self
            .store
            .session_profile(&session_id)
            .map_err(DeaconError::Store)?;
        let light = self.light_profile && !escalated && profile.as_deref() == Some("light");
        // Restore the folder this session opened, so a resumed coding session
        // keeps editing the same tree (and stays jailed to it) instead of
        // silently dropping back to the deacon's cwd.
        let workspace = self
            .store
            .session_workspace(&session_id)
            .map_err(DeaconError::Store)?
            .map(std::path::PathBuf::from);
        let (mut catalog, review_catalog, mut ledger) = self
            .make_catalogs_and_prompt(&provider, &sid_cell, key, None, light)
            .await?;
        let escalate_pending = Arc::new(std::sync::atomic::AtomicBool::new(false));
        if light {
            catalog.add_hook(Arc::new(super::hooks::EscalationHook {
                pending: Arc::clone(&escalate_pending),
            }));
        }
        ledger.seal(&serde_json::to_string(&catalog.definitions()).unwrap_or_default());
        let system_prompt = ledger.render();
        let ctx = self.tool_context(key.is_some(), approval, workspace.as_deref());
        let agent = Agent::resume(
            Arc::clone(&provider),
            Arc::new(catalog),
            Arc::clone(&self.store),
            ctx,
            system_prompt,
            // Resume is the chat surfaces' entry point (desktop rail, CLI,
            // keyed platform conversations); the agent's own child sessions are
            // never resumed, they run once and end.
            self.agent_config(SessionKind::Chat),
            session_id.clone(),
        )
        .map_err(DeaconError::Core)?
        .with_graph_memory(Arc::clone(&self.graph))
        .with_background_review(self.review_setup(review_catalog))
        .with_delta_sink(self.delta_sink(&sid_cell));

        // Resume keeps the STORED prompt when it differs from a fresh render;
        // rebase the baseline onto the bytes the agent will actually send so a
        // legitimately different stored prompt never reads as a cache bust.
        ledger.rebase(agent.system_prompt());
        let entry = self.make_entry(
            agent,
            approval_pending,
            ledger,
            light,
            escalate_pending,
            key,
            workspace,
        );
        // Two callers can pass the fast-path check while the session is cold.
        // Whichever finishes first becomes authoritative; never replace it.
        self.entries
            .lock()
            .await
            .entry(session_id.clone())
            .or_insert(entry);
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
            .create_session_keyed(Some(conversation_key), SessionKind::Chat, None, None)
            .await?;
        self.store
            .bind_conversation(conversation_key, &sid.to_string())?;
        Ok(sid)
    }
}

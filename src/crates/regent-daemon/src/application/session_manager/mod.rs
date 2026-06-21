//! Per-session Agent lifecycle: create, resume, run_turn, drain. One agent
//! mutex per session — one turn at a time. Construction helpers live in
//! `build`, the per-session plumbing types in `hooks`, and the read/approval
//! accessors in `queries`.

mod build;
mod hooks;
mod queries;

use crate::domain::contracts::{OutboundTx, ProviderFactory};
use crate::domain::errors::DaemonError;
use hooks::{RpcApprovalHandler, SessionEntry};
use regent_agent::{Agent, AgentConfig};
use regent_kernel::SessionId;
use regent_skills::SkillLibrary;
use regent_store::Store;
use regent_tools::ToolContext;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::{Mutex, oneshot};
use tokio_util::sync::CancellationToken;

pub struct SessionManager {
    /// Builds a provider for a model id; new sessions use the current model.
    provider_factory: ProviderFactory,
    /// Active model for new sessions — mutated by `set_model`.
    current_model: std::sync::Mutex<String>,
    store: Arc<Store>,
    graph: Arc<regent_graph::GraphMemory>,
    skills: Arc<SkillLibrary>,
    cwd: PathBuf,
    /// Template cloned per session (source overridden to "daemon"); built from
    /// config.yaml at the composition root — the single behavior source.
    agent_template: AgentConfig,
    /// Tool names filtered out of every session catalog (config `tools.disabled`).
    disabled_tools: Vec<String>,
    entries: Mutex<HashMap<SessionId, SessionEntry>>,
    out_tx: OutboundTx,
}

impl SessionManager {
    // Composition-root wiring — all dependencies arrive explicitly.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider_factory: ProviderFactory,
        initial_model: impl Into<String>,
        store: Arc<Store>,
        graph: Arc<regent_graph::GraphMemory>,
        skills: Arc<SkillLibrary>,
        cwd: PathBuf,
        agent_template: AgentConfig,
        disabled_tools: Vec<String>,
        out_tx: OutboundTx,
    ) -> Self {
        Self {
            provider_factory,
            current_model: std::sync::Mutex::new(initial_model.into()),
            store,
            graph,
            skills,
            cwd,
            agent_template,
            disabled_tools,
            entries: Mutex::new(HashMap::new()),
            out_tx,
        }
    }

    pub async fn create_session(&self) -> Result<SessionId, DaemonError> {
        let sid_cell: Arc<OnceLock<String>> = Arc::new(OnceLock::new());
        let approval_pending: Arc<Mutex<Option<oneshot::Sender<bool>>>> =
            Arc::new(Mutex::new(None));
        let approval = Arc::new(RpcApprovalHandler {
            session_id: Arc::clone(&sid_cell),
            out_tx: self.out_tx.clone(),
            pending: Arc::clone(&approval_pending),
        });
        let provider = self.provider();
        let (catalog, review_catalog, system_prompt) =
            self.make_catalogs_and_prompt(&provider, &sid_cell).await?;
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
        .map_err(DaemonError::Core)?
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

    pub async fn resume_session(&self, session_id: SessionId) -> Result<SessionId, DaemonError> {
        self.store
            .session_meta(&session_id)
            .map_err(DaemonError::Store)?;

        let sid_cell: Arc<OnceLock<String>> = Arc::new(OnceLock::new());
        let _ = sid_cell.set(session_id.to_string());
        let approval_pending: Arc<Mutex<Option<oneshot::Sender<bool>>>> =
            Arc::new(Mutex::new(None));
        let approval = Arc::new(RpcApprovalHandler {
            session_id: Arc::clone(&sid_cell),
            out_tx: self.out_tx.clone(),
            pending: Arc::clone(&approval_pending),
        });
        let provider = self.provider();
        let (catalog, review_catalog, system_prompt) =
            self.make_catalogs_and_prompt(&provider, &sid_cell).await?;
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
        .map_err(DaemonError::Core)?
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
    ) -> Result<SessionId, DaemonError> {
        if let Some(stored) = self.store.conversation_session(conversation_key)? {
            let sid = SessionId::from_string(&stored);
            // Already live in memory → reuse it.
            if self.entries.lock().await.contains_key(&sid) {
                return Ok(sid);
            }
            // Bound but cold → resume it (also validates it still exists).
            if let Ok(resumed) = self.resume_session(sid).await {
                return Ok(resumed);
            }
            // Stale binding (session purged) → fall through and recreate.
        }
        let sid = self.create_session().await?;
        self.store
            .bind_conversation(conversation_key, &sid.to_string())?;
        Ok(sid)
    }

    pub async fn run_turn(
        &self,
        session_id: &SessionId,
        text: &str,
    ) -> Result<String, DaemonError> {
        let (agent_arc, interrupt_arc) = {
            let entries = self.entries.lock().await;
            match entries.get(session_id) {
                Some(e) => (Arc::clone(&e.agent), Arc::clone(&e.interrupt)),
                None => return Err(DaemonError::SessionNotFound(session_id.to_string())),
            }
        };

        let mut agent = agent_arc.lock().await;
        agent.reset_interrupt();
        let agent_cancel = agent.cancel_handle();

        let session_cancel = CancellationToken::new();
        *interrupt_arc.lock().await = Some(session_cancel.clone());

        let watcher = tokio::spawn(async move {
            session_cancel.cancelled().await;
            agent_cancel.cancel();
        });

        let result = agent.run_turn(text).await;
        watcher.abort();
        *interrupt_arc.lock().await = None;
        result.map_err(DaemonError::Core)
    }

    /// Cancels every in-flight turn, then waits briefly so cancelled turns
    /// finish recording their ledger rows before the process exits.
    pub async fn drain(&self) {
        let arcs: Vec<_> = {
            let entries = self.entries.lock().await;
            entries.values().map(|e| Arc::clone(&e.interrupt)).collect()
        };
        let mut cancelled_any = false;
        for arc in arcs {
            if let Some(token) = arc.lock().await.as_ref() {
                token.cancel();
                cancelled_any = true;
            }
        }
        if cancelled_any {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
}

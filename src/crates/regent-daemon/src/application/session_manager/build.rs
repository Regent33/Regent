//! Session construction helpers: provider/model selection, tool-catalog +
//! system-prompt assembly, background-review setup, the streaming delta sink,
//! and registry-entry wrapping. Called by the lifecycle code in `mod.rs`.

use super::SessionManager;
use super::hooks::{NotificationDelivery, RpcToolHook, SessionEntry};
use crate::domain::entities::RpcNotification;
use crate::domain::errors::DaemonError;
use regent_agent::{Agent, AgentConfig, DelegateTool, DelegationConfig, ReviewSetup};
use regent_kernel::RegentError;
use regent_providers::ChatProvider;
use regent_skills::REVIEW_SYSTEM_PROMPT;
use regent_tools::{
    ToolCatalog, core_catalog_from_env, register_kanban_tool, register_memory_tools,
    register_message_tool, register_skill_tools,
};
use serde_json::json;
use std::sync::{Arc, OnceLock};
use tokio::sync::{Mutex, oneshot};

/// Board every daemon session shares (multi-tenant boards come with P6's
/// dispatcher); the agent is its own worker until then.
const DAEMON_BOARD: &str = "default";

// kept inline with the assembly that injects it (the single behavior source).
const BASE_PROMPT: &str = "You are Regent, a kind, thoughtful, and warm AI agent. \
You genuinely care about the person you're helping: acknowledge how they're doing, \
celebrate their wins, and be gentle when things go wrong. Use a few well-placed \
emojis to bring warmth (1-3 per reply — never walls of them). Stay capable and \
direct underneath the warmth: use your tools to take action, keep replies focused, \
and never let friendliness pad out the answer.";

impl SessionManager {
    pub(super) fn agent_config(&self) -> AgentConfig {
        AgentConfig { source: "daemon".to_owned(), ..self.agent_template.clone() }
    }

    /// Builds a provider for the current model (a fresh instance per session).
    pub(super) fn provider(&self) -> Arc<dyn ChatProvider> {
        (self.provider_factory)(&self.current_model.lock().unwrap())
    }

    pub(super) fn make_catalogs_and_prompt(
        &self,
        provider: &Arc<dyn ChatProvider>,
        sid_cell: &Arc<OnceLock<String>>,
    ) -> Result<(ToolCatalog, ToolCatalog, String), DaemonError> {
        let mut catalog = core_catalog_from_env().map_err(DaemonError::Core)?;
        register_memory_tools(&mut catalog, Arc::clone(&self.graph), Arc::clone(&self.store))
            .map_err(DaemonError::Core)?;
        register_skill_tools(&mut catalog, Arc::clone(&self.skills)).map_err(DaemonError::Core)?;
        DelegateTool::new(
            Arc::clone(provider),
            Arc::clone(&self.store),
            Arc::new(core_catalog_from_env().map_err(DaemonError::Core)?),
            DelegationConfig::default(),
        )
        .register(&mut catalog)
        .map_err(DaemonError::Core)?;
        // Proactive delivery (send_message → message.outbound) + the kanban
        // worker toolset over the shared board.
        register_message_tool(
            &mut catalog,
            Arc::new(NotificationDelivery {
                session_id: Arc::clone(sid_cell),
                out_tx: self.out_tx.clone(),
            }),
        )
        .map_err(DaemonError::Core)?;
        register_kanban_tool(
            &mut catalog,
            Arc::clone(&self.store),
            DAEMON_BOARD.to_owned(),
            "regent".to_owned(),
        )
        .map_err(DaemonError::Core)?;
        // Per-surface disable: drop config `tools.disabled` from the agent's catalog.
        catalog.disable(&self.disabled_tools);
        catalog.add_hook(Arc::new(RpcToolHook {
            session_id: Arc::clone(sid_cell),
            out_tx: self.out_tx.clone(),
        }));

        let mut review_catalog = ToolCatalog::new();
        register_memory_tools(&mut review_catalog, Arc::clone(&self.graph), Arc::clone(&self.store))
            .map_err(DaemonError::Core)?;
        register_skill_tools(&mut review_catalog, Arc::clone(&self.skills))
            .map_err(DaemonError::Core)?;

        let system_prompt = format!(
            "{BASE_PROMPT}\n\n{}\n\n{}",
            self.skills.render_index().map_err(RegentError::from).map_err(DaemonError::Core)?,
            self.graph
                .render_prompt_block()
                .map_err(RegentError::from)
                .map_err(DaemonError::Core)?,
        );
        Ok((catalog, review_catalog, system_prompt))
    }

    pub(super) fn review_setup(review_catalog: ToolCatalog) -> ReviewSetup {
        ReviewSetup {
            catalog: Arc::new(review_catalog),
            system_prompt: REVIEW_SYSTEM_PROMPT.to_owned(),
            max_iterations: 8,
        }
    }

    pub(super) fn make_entry(
        &self,
        agent: Agent,
        approval_pending: Arc<Mutex<Option<oneshot::Sender<bool>>>>,
    ) -> SessionEntry {
        SessionEntry {
            agent: Arc::new(Mutex::new(agent)),
            interrupt: Arc::new(Mutex::new(None)),
            approval_pending,
        }
    }

    /// Builds a delta sink that forwards streamed assistant text as
    /// `message.delta` notifications. Reads the session id from the cell at
    /// emit time, so it works even though the id is filled after construction.
    pub(super) fn delta_sink(&self, sid_cell: &Arc<OnceLock<String>>) -> regent_agent::DeltaSink {
        let sid_cell = Arc::clone(sid_cell);
        let out_tx = self.out_tx.clone();
        Arc::new(move |fragment: &str| {
            let sid = sid_cell.get().cloned().unwrap_or_default();
            let notif =
                RpcNotification::new("message.delta", json!({ "session_id": sid, "text": fragment }));
            if let Ok(line) = serde_json::to_string(&notif) {
                out_tx.send(line).ok();
            }
        })
    }
}

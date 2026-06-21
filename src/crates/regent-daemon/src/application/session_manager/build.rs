//! Session construction helpers: provider/model selection, tool-catalog +
//! system-prompt assembly, background-review setup, the streaming delta sink,
//! and registry-entry wrapping. Called by the lifecycle code in `mod.rs`.

use super::SessionManager;
use super::hooks::{NotificationDelivery, RpcToolHook, SessionEntry};
use crate::domain::entities::RpcNotification;
use crate::domain::errors::DaemonError;
use regent_agent::{Agent, AgentConfig, BASE_PROMPT, DelegateTool, DelegationConfig, ReviewSetup};
use regent_kernel::RegentError;
use regent_providers::ChatProvider;
use regent_skills::REVIEW_SYSTEM_PROMPT;
use regent_tools::{
    ToolCatalog, core_catalog_from_env, register_kanban_tool, register_key_tool,
    register_memory_tools, register_message_tool, register_persona_tool, register_skill_tools,
};
use serde_json::json;
use std::sync::{Arc, OnceLock};
use tokio::sync::{Mutex, oneshot};

/// Board every daemon session shares (multi-tenant boards come with P6's
/// dispatcher); the agent is its own worker until then.
const DAEMON_BOARD: &str = "default";

/// "\n\nThe current date and time is …" from the REGENT_NOW env the CLI sets at
/// spawn (the daemon has no clock dep) — injected once at session build so the
/// agent can answer date/time immediately, without mutating the cached prompt
/// mid-turn. Empty when unset.
fn now_line() -> String {
    std::env::var("REGENT_NOW")
        .ok()
        .filter(|n| !n.is_empty())
        .map(|n| format!("\n\nThe current date and time is {n} (the user's local time)."))
        .unwrap_or_default()
}

/// Directive pointing the agent at a per-object artifacts area under `.regent`
/// (the `REGENT_HOME` the CLI passes at spawn). Generated standalone
/// artifacts/projects each get their own subfolder there — distinct from edits
/// to the user's existing files. Empty when `REGENT_HOME` is unset.
fn artifacts_line() -> String {
    std::env::var("REGENT_HOME")
        .ok()
        .filter(|h| !h.is_empty())
        .map(|h| {
            let dir = std::path::Path::new(&h).join("artifacts");
            format!(
                "\n\nWhen you generate a new standalone artifact or project (not edits to the \
                 user's existing files), create a dedicated folder for it under {} — one subfolder \
                 per object, e.g. {}{}<short-slug>/ — put its files there, and tell the user the \
                 path. Use the user's working directory only for changes to their existing project.",
                dir.display(),
                dir.display(),
                std::path::MAIN_SEPARATOR,
            )
        })
        .unwrap_or_default()
}

impl SessionManager {
    pub(super) fn agent_config(&self) -> AgentConfig {
        AgentConfig {
            source: "daemon".to_owned(),
            ..self.agent_template.clone()
        }
    }

    /// Builds a provider for the current model (a fresh instance per session).
    pub(super) fn provider(&self) -> Arc<dyn ChatProvider> {
        (self.provider_factory)(&self.current_model.lock().unwrap())
    }

    pub(super) async fn make_catalogs_and_prompt(
        &self,
        provider: &Arc<dyn ChatProvider>,
        sid_cell: &Arc<OnceLock<String>>,
    ) -> Result<(ToolCatalog, ToolCatalog, String), DaemonError> {
        let mut catalog = core_catalog_from_env().map_err(DaemonError::Core)?;
        register_memory_tools(
            &mut catalog,
            Arc::clone(&self.graph),
            Arc::clone(&self.store),
        )
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
        register_persona_tool(&mut catalog, Arc::clone(&self.store)).map_err(DaemonError::Core)?;
        register_key_tool(&mut catalog).map_err(DaemonError::Core)?;
        // Browser control via an external Playwright MCP server (opt-in via
        // REGENT_BROWSER_MCP_URL); best-effort, mutating actions approval-gated.
        regent_tools::attach_browser_if_configured(&mut catalog).await;
        // Per-surface disable: drop config `tools.disabled` from the agent's catalog.
        catalog.disable(&self.disabled_tools);
        catalog.add_hook(Arc::new(RpcToolHook {
            session_id: Arc::clone(sid_cell),
            out_tx: self.out_tx.clone(),
        }));

        let mut review_catalog = ToolCatalog::new();
        register_memory_tools(
            &mut review_catalog,
            Arc::clone(&self.graph),
            Arc::clone(&self.store),
        )
        .map_err(DaemonError::Core)?;
        register_skill_tools(&mut review_catalog, Arc::clone(&self.skills))
            .map_err(DaemonError::Core)?;

        let system_prompt = format!(
            "{BASE_PROMPT}{}{}{}\n\n{}\n\n{}",
            now_line(),
            artifacts_line(),
            self.store.persona_block(),
            self.skills
                .render_index()
                .map_err(RegentError::from)
                .map_err(DaemonError::Core)?,
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
            let notif = RpcNotification::new(
                "message.delta",
                json!({ "session_id": sid, "text": fragment }),
            );
            if let Ok(line) = serde_json::to_string(&notif) {
                out_tx.send(line).ok();
            }
        })
    }
}

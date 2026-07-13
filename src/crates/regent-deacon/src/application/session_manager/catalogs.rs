//! Main-catalog assembly: every tool a session gets, and the `tools.list`
//! view of the same set. Split from `build.rs` (file-size rule) — same
//! `SessionManager`, extension `impl`.

use super::SessionManager;
use super::build::DEACON_BOARD;
use super::hooks::NotificationDelivery;
use crate::domain::errors::DeaconError;
use regent_agent::{DelegateTool, DelegationConfig};
use regent_providers::ChatProvider;
use regent_tools::{
    ToolCatalog, core_catalog_from_env, register_file_tool, register_kanban_tool,
    register_key_tool, register_memory_tools, register_message_tool, register_persona_tool,
    register_skill_tools,
};
use std::sync::{Arc, OnceLock};

impl SessionManager {
    /// The full main tool catalog a session gets — core + memory + skills +
    /// delegate + message/file + kanban + persona + keys + the in-process
    /// `regent` tool + (opt-in) browser. Shared by session build and the
    /// `tools.list` listing so both reflect the same set (no drift). The
    /// per-surface `disable` filter + the RPC hook are applied by the caller
    /// (session-only), not here.
    pub(super) async fn build_main_catalog(
        &self,
        provider: &Arc<dyn ChatProvider>,
        sid_cell: &Arc<OnceLock<String>>,
        conversation_key: Option<&str>,
    ) -> Result<ToolCatalog, DeaconError> {
        let mut catalog = core_catalog_from_env().map_err(DeaconError::Core)?;
        register_memory_tools(
            &mut catalog,
            Arc::clone(&self.graph),
            Arc::clone(&self.store),
        )
        .map_err(DeaconError::Core)?;
        register_skill_tools(&mut catalog, Arc::clone(&self.skills)).map_err(DeaconError::Core)?;
        DelegateTool::new(
            Arc::clone(provider),
            Arc::clone(&self.store),
            Arc::new(core_catalog_from_env().map_err(DeaconError::Core)?),
            DelegationConfig::default(),
        )
        .register(&mut catalog)
        .map_err(DeaconError::Core)?;
        // Outbound delivery. A keyed platform session (Slack/WhatsApp/…) routes
        // send_message *and* send_file back to that platform's API; local/CLI
        // sessions notify the connected client (send_message → message.outbound,
        // no file upload path). Plus the kanban worker toolset below.
        match self.platform_sink(conversation_key) {
            Some(sink) => {
                register_message_tool(&mut catalog, Arc::clone(&sink))
                    .map_err(DeaconError::Core)?;
                register_file_tool(&mut catalog, sink).map_err(DeaconError::Core)?;
            }
            None => {
                register_message_tool(
                    &mut catalog,
                    Arc::new(NotificationDelivery {
                        session_id: Arc::clone(sid_cell),
                        out_tx: self.out_tx.clone(),
                    }),
                )
                .map_err(DeaconError::Core)?;
            }
        }
        register_kanban_tool(
            &mut catalog,
            Arc::clone(&self.store),
            DEACON_BOARD.to_owned(),
            "regent".to_owned(),
        )
        .map_err(DeaconError::Core)?;
        register_persona_tool(&mut catalog, Arc::clone(&self.store)).map_err(DeaconError::Core)?;
        register_key_tool(&mut catalog).map_err(DeaconError::Core)?;
        // The in-process `regent` admin tool: the agent runs its OWN commands
        // (model/status/cron/…) through this deacon's dispatcher, never the CLI.
        // Only present once the composition root has installed the self-handle.
        if let Some(weak) = self.self_ref.get().cloned() {
            catalog
                .register(
                    crate::application::regent_tool::definition(),
                    Arc::new(crate::application::regent_tool::RegentCommandTool::new(
                        weak.clone(),
                    )),
                )
                .map_err(DeaconError::Core)?;
            // Automatic coding-harness routing (ADR-027): the model sends
            // nontrivial code changes through plan→execute→verify→revert.
            catalog
                .register(
                    crate::application::code_task_tool::definition(),
                    Arc::new(crate::application::code_task_tool::CodeTaskTool::new(
                        weak.clone(),
                    )),
                )
                .map_err(DeaconError::Core)?;
            // Fire-and-acknowledge for long jobs (builds, research, documents):
            // a detached agent session does the work; results are injected into
            // the next real turn by the dispatcher (see background_task_tool).
            catalog
                .register(
                    crate::application::background_task_tool::definition(),
                    Arc::new(
                        crate::application::background_task_tool::BackgroundTaskTool::new(
                            weak.clone(),
                        ),
                    ),
                )
                .map_err(DeaconError::Core)?;
            // Read-only reconnaissance scout (gap T3): answers "where/how"
            // questions in a child session so the parent context stays lean.
            catalog
                .register(
                    crate::application::explore_tool::definition(),
                    Arc::new(crate::application::explore_tool::ExploreTool::new(weak)),
                )
                .map_err(DeaconError::Core)?;
        }
        // Browser control via an external Playwright MCP server (opt-in via
        // REGENT_BROWSER_MCP_URL); best-effort, mutating actions approval-gated.
        regent_tools::attach_browser_if_configured(&mut catalog).await;
        Ok(catalog)
    }

    /// Every tool the agent has in a session — for `tools.list` / the welcome
    /// panel. Builds the full catalog (fresh id cell, local sink) and returns
    /// its definitions; the caller applies the disabled filter.
    pub async fn list_tool_definitions(
        &self,
    ) -> Result<Vec<regent_kernel::ToolDefinition>, DeaconError> {
        let provider = self.provider();
        let sid_cell = Arc::new(OnceLock::new());
        let catalog = self.build_main_catalog(&provider, &sid_cell, None).await?;
        Ok(catalog.definitions())
    }
}

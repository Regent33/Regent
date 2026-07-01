//! Session construction helpers: provider/model selection, tool-catalog +
//! system-prompt assembly, background-review setup, the streaming delta sink,
//! and registry-entry wrapping. Called by the lifecycle code in `mod.rs`.

use super::SessionManager;
use super::hooks::{NotificationDelivery, RpcToolHook, SessionEntry};
use crate::domain::entities::RpcNotification;
use crate::domain::errors::DeaconError;
use regent_agent::{
    Agent, AgentConfig, BASE_PROMPT, CAPABILITIES, DelegateTool, DelegationConfig, ReviewSetup,
};
use regent_kernel::RegentError;
use regent_providers::ChatProvider;
use regent_skills::REVIEW_SYSTEM_PROMPT;
use regent_tools::{
    ToolCatalog, core_catalog_from_env, register_file_tool, register_kanban_tool,
    register_key_tool, register_memory_tools, register_message_tool, register_persona_tool,
    register_skill_tools,
};
use serde_json::json;
use std::sync::{Arc, OnceLock};
use tokio::sync::{Mutex, oneshot};

/// Board every deacon session shares (multi-tenant boards come with P6's
/// dispatcher); the agent is its own worker until then.
const DEACON_BOARD: &str = "default";

/// "\n\nThe current date and time is …" from the REGENT_NOW env the CLI sets at
/// spawn (the deacon has no clock dep) — injected once at session build so the
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

/// Spoken-style directive for live voice calls. The speech server spawns its
/// deacon with `REGENT_VOICE=1`; that session then answers conversationally
/// (read aloud, not on screen). Text chat has no env → empty → unchanged.
fn voice_line() -> String {
    let on = std::env::var("REGENT_VOICE")
        .map(|v| matches!(v.trim(), "1" | "true" | "TRUE" | "yes"))
        .unwrap_or(false);
    if !on {
        return String::new();
    }
    "\n\nYOU ARE ON A LIVE VOICE CALL. Your reply is read aloud by text-to-speech, so talk like a \
     person on the phone — warm, natural, with contractions. Give the gist in 1-3 short spoken \
     sentences, not a written report. NEVER use markdown, headings, bullet or numbered lists, \
     tables, links, code blocks, a 'References' list, or emoji — none of that can be spoken. If the \
     honest answer is long or list-like (a weather breakdown, search results, many items), say the \
     one-line takeaway and offer to drop the full details in text/chat. Prefer round numbers and \
     plain phrasing over exact figures and jargon. This overrides any formatting guidance above."
        .to_owned()
}

impl SessionManager {
    pub(super) fn agent_config(&self) -> AgentConfig {
        AgentConfig {
            source: "deacon".to_owned(),
            ..self.agent_template.clone()
        }
    }

    /// Builds a provider for the current model (a fresh instance per session).
    pub(super) fn provider(&self) -> Arc<dyn ChatProvider> {
        (self.provider_factory)(&self.current_model.lock().unwrap())
    }

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
                        weak,
                    )),
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

    pub(super) async fn make_catalogs_and_prompt(
        &self,
        provider: &Arc<dyn ChatProvider>,
        sid_cell: &Arc<OnceLock<String>>,
        conversation_key: Option<&str>,
    ) -> Result<(ToolCatalog, ToolCatalog, String), DeaconError> {
        let mut catalog = self
            .build_main_catalog(provider, sid_cell, conversation_key)
            .await?;
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
        .map_err(DeaconError::Core)?;
        register_skill_tools(&mut review_catalog, Arc::clone(&self.skills))
            .map_err(DeaconError::Core)?;
        register_persona_tool(&mut review_catalog, Arc::clone(&self.store))
            .map_err(DeaconError::Core)?;

        let system_prompt = format!(
            "{BASE_PROMPT}{}{}{}\n\n{CAPABILITIES}\n\n{}\n\n{}{}",
            now_line(),
            artifacts_line(),
            self.store.persona_block(),
            self.skills
                .render_index()
                .map_err(RegentError::from)
                .map_err(DeaconError::Core)?,
            self.graph
                .render_prompt_block()
                .map_err(RegentError::from)
                .map_err(DeaconError::Core)?,
            // Trailing so it's the most salient — overrides text-formatting habits
            // for voice sessions; empty (no-op) for text chat.
            voice_line(),
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

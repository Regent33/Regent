//! Session construction helpers: provider/model selection, tool-catalog +
//! system-prompt assembly, background-review setup, the streaming delta sink,
//! and registry-entry wrapping. Called by the lifecycle code in `mod.rs`.

use super::SessionManager;
use super::hooks::{NotificationDelivery, RpcToolHook, SessionEntry};
use crate::domain::entities::RpcNotification;
use crate::domain::errors::DeaconError;
use crate::domain::ledger::{Ledger, Segment};
use regent_agent::{
    Agent, AgentConfig, CAPABILITIES, DelegateTool, DelegationConfig, ReviewSetup, SYSTEM_PROMPT,
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
     plain phrasing over exact figures and jargon. This overrides any formatting guidance above.\
     \n\nControlling the screen by voice: computer_use keys/clicks act on the FOCUSED window, and \
     THIS CALL is running in a browser tab — so a blind 'close this tab' could close the call \
     itself. When the caller says 'this'/'that'/'here' about a window, tab, or app and you can't \
     tell what's in front, take a screenshot first to see the focused window; if it's still \
     ambiguous or you'd be acting on the call tab, ask which one in one short sentence before you \
     act. When it's clearly unambiguous, just do it.\
     \n\nLong jobs on a call: for work that needs more than a minute or two — building or fixing \
     software (code_task included), deep research, producing documents, spreadsheets, or \
     presentations — call background_task instead of doing it inline, tell the caller it's \
     started, and keep the conversation going. The result reaches you automatically in a later \
     turn; speak its takeaway then. Never leave the caller waiting in silence for a long job."
        .to_owned()
}

impl SessionManager {
    pub(super) fn agent_config(&self) -> AgentConfig {
        let source = "deacon".to_owned();
        // SPL P2 cadence gate: resolve the prompt-cache policy from the session
        // SOURCE once at build (the study is the source of truth — see
        // `domain::cache_policy`). `deacon` sessions chain (mean 10.7 turns) so
        // they earn 5m breakpoints; review/delegate/unknown get none.
        let cache_policy = crate::domain::cache_policy::cache_policy_for_source(&source);
        AgentConfig {
            cache_policy,
            source,
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
                        crate::application::background_task_tool::BackgroundTaskTool::new(weak),
                    ),
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

    /// Assembles the session's tool catalogs and its system prompt AS A LEDGER
    /// (SPL §3.1): the same bytes as ever — `ledger.render()` reproduces the
    /// historical `format!` concatenation exactly — but each segment now
    /// carries its stability tier so per-turn telemetry can catch a
    /// cache-busting regression the day it's introduced. The caller seals the
    /// ledger once the catalog is final and renders the prompt from it.
    pub(super) async fn make_catalogs_and_prompt(
        &self,
        provider: &Arc<dyn ChatProvider>,
        sid_cell: &Arc<OnceLock<String>>,
        conversation_key: Option<&str>,
    ) -> Result<(ToolCatalog, ToolCatalog, Ledger), DeaconError> {
        let mut catalog = self
            .build_main_catalog(provider, sid_cell, conversation_key)
            .await?;
        // Per-surface disable: drop config `tools.disabled` from the agent's catalog.
        catalog.disable(&self.disabled_tools);
        // Token efficiency: withhold rare tools' schemas until loaded
        // (config `tools.deferred`; capability preserved via `load_tools`).
        catalog
            .defer(&self.deferred_tools)
            .map_err(DeaconError::Core)?;
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

        let skills_index = self
            .skills
            .render_index()
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)?;
        let memory_block = self
            .graph
            .render_prompt_block()
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)?;
        // Segment order and separators must stay byte-identical to the former
        // `format!("{SYSTEM_PROMPT}{now}{artifacts}{persona}\n\n{CAPABILITIES}
        // \n\n{skills}\n\n{memory}{voice}")` — separators ride the segment they
        // precede. Env-derived lines are Tier 0 because the env is read once at
        // spawn; a "fix" to live wall-clock would bust the cache every turn.
        let ledger = Ledger::new(vec![
            Segment::tier0("system_prompt", SYSTEM_PROMPT),
            Segment::tier0("now_line", now_line()),
            Segment::tier0("artifacts_line", artifacts_line()),
            Segment::tier1("persona", self.store.persona_block()),
            Segment::tier0("capabilities", format!("\n\n{CAPABILITIES}")),
            Segment::tier1("skills_index", format!("\n\n{skills_index}")),
            Segment::tier1("memory", format!("\n\n{memory_block}")),
            // Trailing so it's the most salient — overrides text-formatting habits
            // for voice sessions; empty (no-op) for text chat.
            Segment::tier0("voice_line", voice_line()),
        ]);
        Ok((catalog, review_catalog, ledger))
    }

    /// The fixed prefix a NEW session would send before any history: the
    /// rendered system prompt and the serialized tool definitions. Powers the
    /// CI prefix-ceiling gate (SPL §3.3) — and later the `context.budget` op.
    pub async fn fixed_prefix(&self) -> Result<(String, String), DeaconError> {
        let provider = self.provider();
        let sid_cell = Arc::new(OnceLock::new());
        let (catalog, _review, ledger) = self
            .make_catalogs_and_prompt(&provider, &sid_cell, None)
            .await?;
        let defs = serde_json::to_string(&catalog.definitions()).unwrap_or_default();
        Ok((ledger.render(), defs))
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
        ledger: Ledger,
    ) -> SessionEntry {
        SessionEntry {
            agent: Arc::new(Mutex::new(agent)),
            interrupt: Arc::new(Mutex::new(None)),
            approval_pending,
            provider_epoch: Arc::new(std::sync::atomic::AtomicU64::new(self.routing_epoch())),
            ledger: Arc::new(ledger),
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

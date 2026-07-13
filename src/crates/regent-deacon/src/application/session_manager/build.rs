//! Session construction helpers: provider/model selection, tool-catalog +
//! system-prompt assembly, background-review setup, the streaming delta sink,
//! and registry-entry wrapping. Called by the lifecycle code in `mod.rs`.

use super::SessionManager;
use super::hooks::{NotificationDelivery, RpcToolHook, SessionEntry};
use crate::domain::entities::RpcNotification;
use crate::domain::errors::DeaconError;
use crate::domain::ledger::{Ledger, Segment, Tier};
use regent_agent::{
    Agent, AgentConfig, CAPABILITIES, DelegateTool, DelegationConfig, ReviewSetup, SYSTEM_PROMPT,
    VISUAL_EXPLAINER,
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

/// Usage window for adaptive tool tiering (SPL §3.5): a tool invoked at least
/// once inside it keeps its schema resident; unused tools defer until
/// `load_tools` (or a direct call) promotes them.
const AUTO_TIER_WINDOW_DAYS: f64 = 30.0;

/// Read-side Tier-1 ceiling (SPL §3.4, the ECC cap pattern from §3.7): even
/// when every store sits at its own budget, the SESSION tier injects at most
/// this many chars — three maxed stores can't stack. Sized just above the sum
/// of today's per-store budgets (personas 28k + skills index ~4k + memory
/// ~3.6k), so it only bites when something actually stacks past design.
const TIER1_CEILING_CHARS: usize = 36_000;

/// Trims Tier-1 segments to the ceiling, walking from the END — later
/// segments (memory, skills index) are retrievable on demand via
/// `memory_search`/`skills_list`, while persona renders first and is trimmed
/// last. A partially-trimmed segment gets a marker naming the trim; the
/// marker's ~0.1k overshoot is accepted. Tier-0 segments are never touched.
fn cap_tier1(mut segments: Vec<Segment>) -> Vec<Segment> {
    const MARKER: &str = "\n\n[…session context trimmed at the Tier-1 ceiling — the full \
                          content stays retrievable via memory_search / skills_list]";
    let total: usize = segments
        .iter()
        .filter(|s| s.tier == Tier::Session)
        .map(|s| s.text.len())
        .sum();
    let Some(mut over) = total.checked_sub(TIER1_CEILING_CHARS).filter(|o| *o > 0) else {
        return segments;
    };
    for seg in segments.iter_mut().rev() {
        if seg.tier != Tier::Session || over == 0 {
            continue;
        }
        if seg.text.len() <= over {
            over -= seg.text.len();
            seg.text.clear();
        } else {
            let mut keep = seg.text.len() - over;
            while !seg.text.is_char_boundary(keep) {
                keep -= 1;
            }
            seg.text.truncate(keep);
            seg.text.push_str(MARKER);
            over = 0;
        }
    }
    segments
}

/// "\n\nThe session started …" from the LIVE local clock — injected once at
/// session build so the agent answers date/time immediately without mutating
/// the cached prompt mid-turn. Deliberately NOT the launchers' `REGENT_NOW`
/// env: that's captured once at deacon SPAWN, so a long-lived deacon handed
/// every new session a days-stale date (the bug users hit as "Regent doesn't
/// know the date"). Names itself session-START time and points at the
/// `current_time` tool so long sessions stay honest too.
fn now_line() -> String {
    let now = chrono::Local::now()
        .format("%A, %B %e, %Y at %I:%M %p (UTC%:z)")
        .to_string();
    format!(
        "\n\nThis session started {now} — the user's local time. Time has passed since; when \
         the exact present moment matters, call the current_time tool."
    )
}

/// Directive pointing the agent at the per-object artifacts area under the
/// real `$REGENT_HOME` (env, else `~/.regent` — never a cwd-relative guess:
/// an unset env used to silence this line and the agent then invented a
/// `.regent/` folder inside whatever directory the deacon ran from).
fn artifacts_line() -> String {
    let dir = crate::application::http_serve::regent_home().join("artifacts");
    format!(
        "\n\nWhen you generate a new standalone artifact or file to send (screenshots included — \
         not edits to the user's existing files), create a dedicated folder for it under {} — one \
         subfolder per object, e.g. {}{}<short-slug>/ — put its files there, and tell the user the \
         path. Never create files elsewhere for these; use the user's working directory only for \
         changes to their existing project.",
        dir.display(),
        dir.display(),
        std::path::MAIN_SEPARATOR,
    )
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
     turn; speak its takeaway then. Never leave the caller waiting in silence for a long job.\n\n"
        .to_owned()
        + VISUAL_EXPLAINER
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
        skill_overlay: Option<&str>,
    ) -> Result<(ToolCatalog, ToolCatalog, Ledger), DeaconError> {
        let mut catalog = self
            .build_main_catalog(provider, sid_cell, conversation_key)
            .await?;
        // Per-surface disable: drop config `tools.disabled` from the agent's catalog.
        catalog.disable(&self.disabled_tools);
        // Token efficiency: withhold rare tools' schemas until loaded
        // (config `tools.deferred`; capability preserved via `load_tools`),
        // plus adaptive tiering (SPL §3.5): tools with no recorded use in the
        // last 30 days are deferred too — residency is earned by usage, so
        // catalog growth is pay-when-used. Pinned tools never defer. Computed
        // ONCE here, so the deferred set is stable for the session (a mid-
        // session change would bust the Tier-0 cache).
        let mut deferred = self.deferred_tools.clone();
        // Fail-open: a store read error skips auto-tiering (full catalog).
        if self.auto_tier
            && let Ok(used) = self.store.tool_use_counts(AUTO_TIER_WINDOW_DAYS)
        {
            for def in catalog.definitions() {
                if !used.contains_key(&def.name) && !deferred.contains(&def.name) {
                    deferred.push(def.name);
                }
            }
        }
        deferred.retain(|n| !self.pinned_tools.contains(n));
        catalog.defer(&deferred).map_err(DeaconError::Core)?;
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
        let ledger = Ledger::new(cap_tier1(vec![
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
            // Wave 1c harness-skill seam: a named skill's body, appended for
            // `code.plan`/`code.start` sessions at build (the prompt is frozen
            // per session). Empty (byte-identical render) for every other path.
            Segment::tier0("code_skill", skill_overlay.unwrap_or_default()),
        ]));
        Ok((catalog, review_catalog, ledger))
    }

    /// The fixed prefix a NEW session would send before any history: the
    /// rendered system prompt and the serialized tool definitions. Powers the
    /// CI prefix-ceiling gate (SPL §3.3) — and later the `context.budget` op.
    pub async fn fixed_prefix(&self) -> Result<(String, String), DeaconError> {
        let provider = self.provider();
        let sid_cell = Arc::new(OnceLock::new());
        let (catalog, _review, ledger) = self
            .make_catalogs_and_prompt(&provider, &sid_cell, None, None)
            .await?;
        let defs = serde_json::to_string(&catalog.definitions()).unwrap_or_default();
        Ok((ledger.render(), defs))
    }

    pub(super) fn review_setup(review_catalog: ToolCatalog) -> ReviewSetup {
        ReviewSetup {
            catalog: Arc::new(review_catalog),
            system_prompt: REVIEW_SYSTEM_PROMPT.to_owned(),
            max_iterations: 8,
            // ~2-4 exchanges per review batch instead of one review per turn
            // (the 800-sessions/2wk flood, handoff 2026-07-13).
            min_new_messages: 8,
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

#[cfg(test)]
mod tests {
    use super::{TIER1_CEILING_CHARS, cap_tier1};
    use crate::domain::ledger::{Segment, Tier};

    // SPL §3.4: three maxed stores can't stack — the SESSION tier is capped,
    // trimming from the end (memory before skills before persona), Tier-0
    // segments untouched, and a marker names the trim.
    #[test]
    fn tier1_ceiling_trims_from_the_end_and_spares_tier0() {
        let capped = cap_tier1(vec![
            Segment::tier0("system_prompt", "S".repeat(90_000)),
            Segment::tier1("persona", "P".repeat(28_000)),
            Segment::tier1("skills_index", "K".repeat(6_000)),
            Segment::tier1("memory", "M".repeat(9_000)),
        ]);
        assert_eq!(capped[0].text.len(), 90_000, "Tier 0 is never trimmed");
        assert_eq!(capped[1].text.len(), 28_000, "persona is trimmed last");
        // 43k of Tier 1 → 7k over: memory absorbs the whole trim (9k → 2k +
        // marker), skills survive intact.
        assert_eq!(capped[2].text.len(), 6_000);
        assert!(capped[3].text.starts_with("MM"));
        assert!(capped[3].text.contains("trimmed at the Tier-1 ceiling"));
        let tier1: usize = capped
            .iter()
            .filter(|s| s.tier == Tier::Session)
            .map(|s| s.text.len())
            .sum();
        assert!(
            tier1 <= TIER1_CEILING_CHARS + 200,
            "within ceiling (+marker): {tier1}"
        );

        // Under the ceiling nothing changes.
        let untouched = cap_tier1(vec![Segment::tier1("persona", "p".repeat(100))]);
        assert_eq!(untouched[0].text.len(), 100);
    }
}

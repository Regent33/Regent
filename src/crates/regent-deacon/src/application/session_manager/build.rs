//! Session construction helpers: provider/model selection, tool-catalog +
//! system-prompt assembly, background-review setup, the streaming delta sink,
//! and registry-entry wrapping. Called by the lifecycle code in `mod.rs`.

use super::SessionManager;
use super::hooks::{RpcToolHook, SessionEntry};
#[cfg(test)]
pub(super) use super::prompt_lines::TIER1_CEILING_CHARS;
use super::prompt_lines::{artifacts_line, cap_tier1, now_line, voice_line};
use crate::domain::entities::RpcNotification;
use crate::domain::errors::DeaconError;
use crate::domain::ledger::{Ledger, Segment};
use regent_agent::{
    Agent, AgentConfig, CAPABILITIES, ReviewSetup, SYSTEM_PROMPT,
};
use regent_kernel::RegentError;
use regent_providers::ChatProvider;
use regent_skills::REVIEW_SYSTEM_PROMPT;
use regent_tools::{
    ToolCatalog, register_memory_tools, register_persona_tool,
    register_skill_tools,
};
use serde_json::json;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;

/// Board every deacon session shares (multi-tenant boards come with P6's
/// dispatcher); the agent is its own worker until then.
pub(super) const DEACON_BOARD: &str = "default";

/// Usage window for adaptive tool tiering (SPL §3.5): a tool invoked at least
/// once inside it keeps its schema resident; unused tools defer until
/// `load_tools` (or a direct call) promotes them.
const AUTO_TIER_WINDOW_DAYS: f64 = 30.0;

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
        // Gap S7: user lifecycle hooks observe the same seam (fire-and-forget).
        if let Some(hook) = &self.shell_hook {
            catalog.add_hook(Arc::clone(hook) as Arc<dyn regent_tools::DispatchHook>);
        }

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
        approval_pending: Arc<Mutex<Option<super::hooks::ApprovalTx>>>,
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
#[path = "build_tests.rs"]
mod tests;

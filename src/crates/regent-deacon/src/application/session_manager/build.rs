//! Session construction helpers: provider/model selection, tool-catalog +
//! system-prompt assembly, background-review setup, the streaming delta sink,
//! and registry-entry wrapping. Called by the lifecycle code in `mod.rs`.

use super::SessionManager;
use super::hooks::{RpcToolHook, SessionEntry};
use super::lifecycle::SessionKind;
use super::prompt_lines::{artifacts_line, cap_tier1, now_line, voice_line, voice_session_active};
use crate::domain::entities::RpcNotification;
use crate::domain::errors::DeaconError;
use crate::domain::ledger::{Ledger, Segment};
use regent_agent::{Agent, AgentConfig, CAPABILITIES, ReviewSetup, SYSTEM_PROMPT};
use regent_kernel::RegentError;
use regent_providers::ChatProvider;
use regent_skills::REVIEW_SYSTEM_PROMPT;
use regent_tools::{
    ToolCatalog, register_review_memory_tools, register_review_persona_tool,
    register_review_skill_tools,
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

/// Which tools have not earned their schema's place in every request.
///
/// Residency is priced against turns rather than granted for any use at all.
/// The old rule was "did this tool appear in the window", which is not a
/// threshold: one call in a month kept a full schema resident on every turn for
/// the next thirty. Measured on a real store — 4,152 turns in 30 days —
/// `create_document` spent ~6.2M tokens that way to serve 21 calls, roughly
/// 296k tokens per use, against a `load_tools` hop costing tens.
///
/// Unused is always unearned; otherwise a tool must fall **below** the bar to
/// lose residency, not merely reach it. Landing exactly on the bar keeps a tool
/// — checking this against the real store, `open_url` sat on precisely that
/// boundary, and it is pinned in the default config for the express reason that
/// a `load_tools` round trip is one weak models do not make. A policy whose
/// edge case reintroduces a known regression has the edge case wrong.
///
/// At `min_share = 0.0` the bar is 0, so only genuinely unused tools defer —
/// exactly the old behaviour, which is what makes 0.0 a real escape hatch.
///
/// Pinned tools are removed by the caller, not here: this answers "did it earn
/// its place", and pinning is a separate decision about tools the model must
/// see whether they earned it or not.
///
/// Split out of `build` because it is the whole of the policy and the only part
/// worth testing on its own; the rest of that method needs a store, a provider
/// and a live catalog.
fn unearned(
    names: impl IntoIterator<Item = String>,
    used: &std::collections::HashMap<String, u32>,
    turns: u32,
    min_share: f64,
) -> Vec<String> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let bar = (f64::from(turns) * min_share.max(0.0)).floor() as u32;
    names
        .into_iter()
        .filter(|name| {
            let uses = used.get(name).copied().unwrap_or(0);
            uses == 0 || uses < bar
        })
        .collect()
}

fn must_stay_resident(name: &str, voice_session: bool) -> bool {
    matches!(name, "kanban" | "computer_use")
        || (voice_session && matches!(name, "play" | "control_app"))
}

/// ADR-038 P0(b): the `light` candidate profile's pinned tools — kept
/// resident every turn regardless of usage history; everything else defers
/// to the `load_tools` index. `code_task` stays pinned so the escalation
/// trigger (P2, not yet implemented) is always visible. `load_tools` itself
/// isn't listed here: `ToolCatalog::defer` auto-registers it whenever
/// anything is deferred (see `regent_tools::application::catalog::tiering`).
/// Since P1 this IS the plain-chat catalog (`tools.light_profile` gates it);
/// P2's escalation swaps an affected session back to the full profile.
const LIGHT_PINNED: &[&str] = &[
    "memory_search",
    // The save half of memory. Deferred, "remember this" needed a load_tools
    // escalation weak models never make — in-session learning silently died
    // on light chats (owner repro 2026-07-17). The background review loop
    // covers persona updates; interactive saves need the tool resident.
    "memory",
    // Explicit durable identity/preferences must land in the structured
    // profile immediately. Deferring this tool made weak light-profile models
    // choose `memory` and then falsely claim the persona had been updated.
    "update_persona",
    "session_search",
    // Episodic recall's browse half: "what did we do today?" has no keywords
    // for session_search — without session_list resident, a light chat
    // answered it with "no past sessions" (owner repro 2026-07-17).
    "session_list",
    "current_time",
    // Media requests are direct actions. Deferring `play` made small models
    // mistake song titles for skill names or stall after `load_tools`.
    "play",
    // "pull up <site>" is the same kind of direct action; tiny schema.
    "open_url",
    // Browser/app automation is a direct intent, not an obscure capability.
    // The stable CAPABILITIES segment already says to use it and never deny
    // screen control when it is present. Hiding its schema under light made
    // the model do exactly that, then recover only after the user reminded it
    // and it called load_tools (owner repro 2026-08-17). It is registered only
    // when REGENT_COMPUTER_USE enables it, so this costs nothing when absent.
    "computer_use",
    // The skills index caps itself and points overflow at skills_list. With
    // that tool deferred, the prompt advertised a discovery path weak models
    // could not follow in light chat.
    "skills_list",
    "skill_view",
    "code_task",
    // Every task request now files a board card (see SYSTEM_PROMPT). A rule
    // that fires on EVERY task cannot sit behind a `load_tools` round trip
    // weak light-profile models routinely skip — it would silently never run.
    "kanban",
    // Same reasoning, measured: SYSTEM_PROMPT tells the model that several
    // INDEPENDENT asks in one message must run in parallel through ONE
    // delegate_task call. Deferred, that rule needed two hops a weak model does
    // not make (`load_tools`, then the call), so multi-task requests quietly ran
    // serially or not at all — 6 delegate sessions against 179 background ones
    // on the owner's store. The schema is 628 chars / ~170 tokens, an order of
    // magnitude under `create_document`'s ~1.5k, which is why THAT one must keep
    // deferring and this one need not. Identical trade to `open_url` above.
    "delegate_task",
];

/// The stored `source` for a session of this kind. Only `Chat` is a session a
/// PERSON opened; the rest are children the agent spawns for itself (a
/// `background_task` job, a `code_task` plan/execute run). They persist so they
/// stay inspectable, but a distinct source is what lets the session rail keep
/// them out of the user's chat list — stamped `deacon` they were
/// indistinguishable from a real chat, so kicking off a `background_task` made
/// a "deacon · 44e848" row appear as if Regent had opened a chat with itself.
#[must_use]
pub(super) fn source_for(kind: SessionKind) -> &'static str {
    match kind {
        SessionKind::Chat => "deacon",
        SessionKind::Background => "background",
        SessionKind::CodePlan | SessionKind::CodeExecute => "code",
    }
}

impl SessionManager {
    pub(super) fn agent_config(&self, kind: SessionKind) -> AgentConfig {
        let source = source_for(kind).to_owned();
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

    pub(super) fn provider_for_model(&self, model: &str) -> Arc<dyn ChatProvider> {
        (self.provider_factory)(model)
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
        light: bool,
    ) -> Result<(ToolCatalog, ToolCatalog, Ledger), DeaconError> {
        let mut catalog = self
            .build_main_catalog(provider, sid_cell, conversation_key)
            .await?;
        // Per-surface disable: drop config `tools.disabled` from the agent's catalog.
        catalog.disable(&self.disabled_tools);
        // ADR-038: `light` ignores config `tools.deferred`/auto-tier and the
        // `pinned_tools` safety valve below — LIGHT_PINNED is the light
        // profile's OWN minimal pinned set, deliberately narrower than the
        // config-pinned list (e.g. `read_file`/`terminal` stay resident for
        // `full` but defer under `light`). Set by `fixed_prefix_for` for
        // measurement (P0) and, since P1, by `create_session_keyed` for real
        // plain-chat sessions (`tools.light_profile` gates it).
        let deferred = if light {
            catalog
                .names()
                .into_iter()
                .filter(|n| !LIGHT_PINNED.contains(&n.as_str()))
                .collect()
        } else {
            // Token efficiency: withhold rare tools' schemas until loaded
            // (config `tools.deferred`; capability preserved via `load_tools`),
            // plus adaptive tiering (SPL §3.5): tools with no recorded use in
            // the last 30 days are deferred too — residency is earned by
            // usage, so catalog growth is pay-when-used. Pinned tools never
            // defer. Computed ONCE here, so the deferred set is stable for
            // the session (a mid-session change would bust the Tier-0 cache).
            let mut deferred = self.deferred_tools.clone();
            // Fail-open: a store read error skips auto-tiering (full catalog).
            if self.auto_tier
                && let Ok(used) = self.store.tool_use_counts(AUTO_TIER_WINDOW_DAYS)
            {
                let turns = self
                    .store
                    .assistant_turns(AUTO_TIER_WINDOW_DAYS)
                    .unwrap_or(0);
                let names = catalog.definitions().into_iter().map(|d| d.name);
                for name in unearned(names, &used, turns, self.auto_tier_min_share) {
                    if !deferred.contains(&name) {
                        deferred.push(name);
                    }
                }
                tracing::debug!(
                    turns,
                    deferred = deferred.len(),
                    "tool residency priced against turns"
                );
            }
            deferred.retain(|n| !self.pinned_tools.contains(n));
            deferred
        };
        // A tool the prompt REQUIRES cannot be deferred. This protects kanban's
        // unconditional task rule and computer_use's direct automation rule
        // from both stale user config and adaptive tiering.
        let voice_session = voice_session_active();
        let mut deferred = deferred;
        deferred.retain(|name| !must_stay_resident(name, voice_session));
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
        register_review_memory_tools(
            &mut review_catalog,
            Arc::clone(&self.graph),
            Arc::clone(&self.store),
        )
        .map_err(DeaconError::Core)?;
        register_review_skill_tools(&mut review_catalog, Arc::clone(&self.skills))
            .map_err(DeaconError::Core)?;
        register_review_persona_tool(&mut review_catalog, Arc::clone(&self.store))
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
        // W3 step 1: record what this block costs per session. Opt-in, and a
        // no-op otherwise.
        crate::application::memory_shadow::record_block_cost(&self.graph);
        // Segment order and separators must stay byte-identical to the former
        // `format!("{SYSTEM_PROMPT}{now}{artifacts}{persona}\n\n{CAPABILITIES}
        // \n\n{skills}\n\n{memory}{voice}")` — separators ride the segment they
        // precede. Env-derived lines are Tier 0 because the env is read once at
        // spawn; a "fix" to live wall-clock would bust the cache every turn.
        let mut segments = vec![
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
        ];
        // ADR-038 P1: the light profile LEADS with a one-line header so
        // implicit-cache providers (which route on a hash of roughly the
        // first 256 tokens) keep two independent warm caches for the two
        // profiles. Light-only — the full profile's bytes never shift, so
        // existing sessions' caches and the SPL prefix tests stay untouched.
        if light {
            segments.insert(0, Segment::tier0("profile_header", "profile: light\n\n"));
        }
        let ledger = Ledger::new(cap_tier1(segments));
        Ok((catalog, review_catalog, ledger))
    }

    pub(super) fn review_setup(&self, review_catalog: ToolCatalog) -> ReviewSetup {
        // `model.review`: route the learning loop to a designated (stronger)
        // model instead of whatever serves the chat — a weak chat model
        // grading its own sessions reliably says "Nothing to save".
        let provider = self
            .review_model
            .lock()
            .unwrap()
            .as_ref()
            .filter(|model| !model.trim().is_empty()) // blank == inherit
            .map(|model| (self.provider_factory)(model));
        ReviewSetup {
            catalog: Arc::new(review_catalog),
            system_prompt: REVIEW_SYSTEM_PROMPT.to_owned(),
            max_iterations: 8,
            // ~2-4 exchanges per review batch instead of one review per turn
            // (the 800-sessions/2wk flood, handoff 2026-07-13).
            min_new_messages: 8,
            provider,
        }
    }

    // Registry wiring: every field of one SessionEntry arrives explicitly, same
    // as `SessionManager::new`. Bundling them into a params struct would only
    // move the same list one indirection away.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn make_entry(
        &self,
        agent: Agent,
        approval_pending: Arc<Mutex<Option<super::hooks::ApprovalTx>>>,
        ledger: Ledger,
        light: bool,
        escalate_pending: Arc<std::sync::atomic::AtomicBool>,
        conversation_key: Option<&str>,
        workspace: Option<std::path::PathBuf>,
    ) -> SessionEntry {
        SessionEntry {
            agent: Arc::new(Mutex::new(agent)),
            interrupt: Arc::new(Mutex::new(None)),
            approval_pending,
            provider_epoch: Arc::new(std::sync::atomic::AtomicU64::new(self.routing_epoch())),
            ledger: Arc::new(ledger),
            light: Arc::new(std::sync::atomic::AtomicBool::new(light)),
            escalate_pending,
            conversation_key: conversation_key.map(str::to_owned),
            last_turn_at: Arc::new(std::sync::atomic::AtomicU64::new(super::now_epoch())),
            workspace,
            canary_seen: Arc::default(),
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
#[path = "tests/build.rs"]
mod tests;

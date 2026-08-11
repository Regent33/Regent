//! Post-turn telemetry accessors (context meter, cache usage, reset
//! reasons, per-phase wall clock). Split from `agent/mod.rs` (file-size rule).

use super::*;
use std::time::Instant;

/// Where one turn's wall clock went, in milliseconds. Nothing in the turn path
/// was timed before this, so "Regent feels slow" could only ever be answered
/// with a guess about which phase was to blame.
///
/// Each bucket sums across the turn's iterations, and together they come to
/// LESS than `total_ms`; the remainder is context assembly, transcript
/// bookkeeping and request serialization — all in-memory work, and the
/// residual is the evidence for that. Compaction's own summarizer call counts
/// as `compact_ms`, not `model_ms`: it is a cost of running out of window, not
/// of answering.
///
/// Milliseconds, which means a phase faster than 1ms reports 0 — a local
/// SQLite append usually does. Read a zero as "under the resolution", never as
/// "did not happen". The unit is right for the thing this exists to diagnose
/// (multi-second turns); it is the wrong unit for microbenchmarking a write,
/// and a test asserting a bucket is non-zero will pass or fail on machine speed.
///
/// One overlap, named rather than engineered away: compaction records its
/// summarizer's token usage while `compact_ms` is running, so that single
/// SQLite write is billed to `store_ms` as well. It is one row against a
/// multi-second summarization, and prying it apart would mean threading a
/// "don't bill this" flag through the usage path to buy accuracy in the third
/// decimal place. Read `store_ms` as an upper bound on turns that compacted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TurnTimings {
    /// The whole turn, as the caller experienced it.
    pub total_ms: u64,
    /// Provider completions that answer the user, summed over the loop.
    pub model_ms: u64,
    /// Tool dispatch only — writing the results down is `store_ms`.
    pub tools_ms: u64,
    /// Every SQLite write the turn makes: messages and usage rows.
    pub store_ms: u64,
    /// `maybe_compress`, including its summarizer call and the session split.
    pub compact_ms: u64,
    /// `maybe_prune` + `maybe_collapse`, the in-memory history levers.
    pub levers_ms: u64,
}

/// Milliseconds since `start`, saturating. A turn that outlasts `u64` ms has a
/// problem no measurement will help with.
pub(crate) fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

impl Agent {
    /// Estimated current context size (tokens) and the configured budget — drives
    /// the CLI status line's context-fill bar. Cheap: a length-based estimate, the
    /// same one compression uses, so the two never disagree.
    #[must_use]
    pub fn context_usage(&self) -> (u32, u32) {
        let used = crate::domain::compression::estimate_tokens(
            &self.system_prompt,
            self.transcript.messages(),
        );
        (
            used + self.tool_schema_tokens(),
            self.effective_max_context(),
        )
    }

    /// What the tool catalog costs on the wire.
    ///
    /// Tool definitions are **not** part of the system prompt — they ride their
    /// own `tools` array — so a meter built from `system_prompt + transcript`
    /// could not see them. Measured: the core catalog alone is 30 tools and
    /// ~5,200 tokens (`create_document` is 1,495 of them by itself), before the
    /// deacon adds memory, skills, kanban, persona, keys and the rest. That was
    /// unaccounted for on *every* turn — a bill the user was charged and never
    /// shown, and headroom compaction never knew to reserve.
    ///
    /// Re-derived per call rather than cached, because `escalate_profile` swaps
    /// the catalog mid-session. Serializing a few dozen schemas costs
    /// microseconds against a model call.
    #[must_use]
    pub fn tool_schema_tokens(&self) -> u32 {
        let chars: usize = self
            .catalog
            .definitions()
            .iter()
            .map(|d| {
                // + framing: the JSON wrapper each definition sits in.
                d.name.len() + d.description.len() + d.parameters.to_string().len() + 24
            })
            .sum();
        u32::try_from(chars / 4).unwrap_or(u32::MAX)
    }

    /// The context window to size compaction and the context meter against, in
    /// priority order: the user's per-model override (`context.windows` — the
    /// stale-table escape hatch and the only way to size a local model), the
    /// ACTIVE provider's known family window, then the configured
    /// `max_context_tokens` fallback. `self.provider` is the live handle —
    /// swapped by `set_provider` on a routing-epoch bump, and `FallbackChat`
    /// reports the active chain member — so every rung follows whoever actually
    /// serves the turn: a 256k primary that fails over to a 32k local model
    /// stops doing 128k math on the next turn (ADR-038 P0a).
    #[must_use]
    pub(crate) fn effective_max_context(&self) -> u32 {
        self.config
            .context_windows
            .get(self.provider.model())
            .copied()
            .or_else(|| self.provider.context_window())
            .unwrap_or(self.config.max_context_tokens)
    }

    /// The estimate at which compaction fires for this session, in tokens —
    /// the landmark a context meter should mark, because crossing it summarizes
    /// the history and splits the session into a child. `None` when compaction
    /// cannot fire at all: disabled by config, or the circuit breaker is open
    /// after a pass that failed to shrink below it.
    ///
    /// The single source of the formula — `maybe_compress` reads it too, so the
    /// number shown and the number enforced can never drift apart.
    #[must_use]
    pub fn compaction_threshold(&self) -> Option<u32> {
        let settings = &self.config.compression;
        if !settings.enabled || self.compression_broken {
            return None;
        }
        Some((self.effective_max_context() as f64 * settings.trigger_fraction) as u32)
    }

    /// Prompt/completion tokens the last completed turn spent (summed across its
    /// model calls). `(0, 0)` before the first turn.
    #[must_use]
    pub fn last_turn_usage(&self) -> (u32, u32) {
        (self.last_turn_input_tokens, self.last_turn_output_tokens)
    }

    /// Per-phase wall clock for the current/last turn. All zero before the
    /// first turn, and reset at the top of every turn like the token counters.
    #[must_use]
    pub fn last_turn_timings(&self) -> TurnTimings {
        self.timings
    }

    /// Whether every successful model response in the turn reported usage.
    #[must_use]
    pub fn last_turn_usage_complete(&self) -> bool {
        self.last_turn_usage_complete
    }

    /// Provider-reported input size of the final request in the turn. `None`
    /// means the provider omitted usage; it is never replaced by an estimate.
    #[must_use]
    pub fn last_request_input_tokens(&self) -> Option<u32> {
        self.last_request_input_tokens
    }

    /// SPL P2 (§3.3): provider-reported prompt-cache usage for the last turn as
    /// `(cache_read, cache_write)`. Each is `None` when no model call this turn
    /// reported that field — passed through additively to `turn.complete`.
    #[must_use]
    pub fn last_turn_cache_usage(&self) -> (Option<u32>, Option<u32>) {
        (self.last_turn_cache_read, self.last_turn_cache_write)
    }

    /// SPL cache-reset reason for the current/last turn (`"pruning"`,
    /// `"compaction"`, `"failover"`, `"routing"`, or `"tiering"`; `None` when the
    /// prefix carried over). The seam P2's `cache_reset` attribution reads at
    /// turn end.
    #[must_use]
    pub fn last_cache_reset(&self) -> Option<&'static str> {
        self.last_cache_reset
    }

    /// Records a cache-reset reason for the current turn, keeping the
    /// highest-priority cause when several fire: tiering (Tier-0 definitions
    /// grew) > routing (provider/prefix swap) > compaction (history rewritten) >
    /// failover > pruning. The single write path for `last_cache_reset`.
    ///
    /// `tiering` must outrank EVERY other cause: the deacon uses this reason to
    /// rebase the definitions ledger, so even a simultaneous routing reset must
    /// not mask it.
    pub(crate) fn note_cache_reset(&mut self, reason: &'static str) {
        fn rank(reason: &str) -> u8 {
            match reason {
                "tiering" => 5,
                "routing" => 4,
                "compaction" => 3,
                "failover" => 2,
                "pruning" => 1,
                _ => 0,
            }
        }
        if self
            .last_cache_reset
            .is_none_or(|cur| rank(reason) > rank(cur))
        {
            self.last_cache_reset = Some(reason);
        }
    }

    /// SPL P2: stamps the next turn as a `routing` reset — called by the deacon
    /// when a routing-epoch bump swaps this session's provider before the turn
    /// runs (the whole prefix warms cold on the new model). Consumed at turn
    /// start; harmless if the turn never runs.
    pub fn mark_provider_routed(&mut self) {
        self.pending_cache_reset = Some("routing");
    }
}

//! Post-turn telemetry accessors (context meter, cache usage, reset
//! reasons). Split from `agent/mod.rs` (file-size rule).

use super::*;

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
        (used, self.config.max_context_tokens)
    }

    /// Prompt/completion tokens the last completed turn spent (summed across its
    /// model calls). `(0, 0)` before the first turn.
    #[must_use]
    pub fn last_turn_usage(&self) -> (u32, u32) {
        (self.last_turn_input_tokens, self.last_turn_output_tokens)
    }

    /// SPL P2 (§3.3): provider-reported prompt-cache usage for the last turn as
    /// `(cache_read, cache_write)`. Each is `None` when no model call this turn
    /// reported that field — passed through additively to `turn.complete`.
    #[must_use]
    pub fn last_turn_cache_usage(&self) -> (Option<u32>, Option<u32>) {
        (self.last_turn_cache_read, self.last_turn_cache_write)
    }

    /// SPL cache-reset reason for the current/last turn (`"pruning"`,
    /// `"compaction"`, `"failover"`, or `"routing"`; `None` when the prefix
    /// carried over). The seam P2's `cache_reset` attribution reads at turn end.
    #[must_use]
    pub fn last_cache_reset(&self) -> Option<&'static str> {
        self.last_cache_reset
    }

    /// Records a cache-reset reason for the current turn, keeping the
    /// highest-priority cause when several fire: routing (whole prefix cold) >
    /// compaction (history rewritten wholesale) > failover (provider swapped
    /// mid-turn) > pruning (Tier-2 stub). The single write path for
    /// `last_cache_reset` — every trigger point calls this.
    pub(crate) fn note_cache_reset(&mut self, reason: &'static str) {
        fn rank(reason: &str) -> u8 {
            match reason {
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

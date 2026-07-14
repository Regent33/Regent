//! History-side context levers below compaction: result pruning and
//! argument collapse. Split from `lifecycle.rs` (file-size rule).

use crate::application::agent::Agent;
use crate::domain::compression;
use regent_kernel::Transcript;

impl Agent {
    /// Tool-result pruning (SPL §3.8), the history-side lever. Runs at the same
    /// decision point as compaction, *before* it: it replaces stale tool-result
    /// content with a stub, shrinking Tier-2 history so the compaction estimate
    /// stays under threshold longer (pruning delays the wholesale rewrite). Only
    /// tool results are touched; the newest `protect_last_n` are spared; a prune
    /// fires only when the reclaimable volume clears the batch floor, so each one
    /// pays for the cache reset it forces. Pure decision in `compression`; here we
    /// just rebuild the transcript — only tool-result *content* changed, so every
    /// push re-validates structurally identically to before.
    pub(crate) fn maybe_prune(&mut self) {
        let settings = &self.config.compression;
        if !settings.enabled {
            return;
        }
        let Some(pruned) = compression::prune_tool_results(
            self.transcript.messages(),
            settings.prune_after_turns,
            settings.protect_last_n,
        ) else {
            return;
        };
        let before = self.transcript.messages().len();
        let mut rebuilt = Transcript::new();
        for message in pruned {
            // Content-only rewrite: structure is unchanged, so this cannot fail.
            // If it somehow did, abandon the prune rather than corrupt history.
            if rebuilt.push(message).is_err() {
                tracing::warn!("prune rebuild violated transcript order — skipping prune");
                return;
            }
        }
        self.transcript = rebuilt;
        // SPL cache-reset seam: this turn busts the Tier-2 cache; reason = pruning
        // (lowest priority — compaction/failover/routing override it if they fire).
        self.note_cache_reset("pruning");
        tracing::info!(
            messages = before,
            "tool-result pruning fired (cache_reset: pruning)"
        );
    }

    /// Mid-tier collapse (gap C3), the second history-side lever: stale tool
    /// EXCHANGES lose their fat tool-call arguments (result-pruning already
    /// stubbed their results). Runs at the same decision point, after pruning
    /// and before compaction; staleness is twice the pruning horizon, so the
    /// tiers stay ordered (results stub first, arguments later, compaction
    /// last). Same batch-floor discipline — a collapse that reclaims scraps
    /// never fires.
    pub(crate) fn maybe_collapse(&mut self) {
        let settings = &self.config.compression;
        if !settings.enabled {
            return;
        }
        let Some(collapsed) = crate::domain::collapse::collapse_tool_exchanges(
            self.transcript.messages(),
            settings.prune_after_turns * 2,
            settings.protect_last_n,
        ) else {
            return;
        };
        let mut rebuilt = Transcript::new();
        for message in collapsed {
            // Arguments-only rewrite: structure unchanged; abandon on any
            // violation rather than corrupt history.
            if rebuilt.push(message).is_err() {
                tracing::warn!("collapse rebuild violated transcript order — skipping collapse");
                return;
            }
        }
        self.transcript = rebuilt;
        self.note_cache_reset("pruning");
        tracing::info!("mid-tier tool-exchange collapse fired (cache_reset: pruning)");
    }
}

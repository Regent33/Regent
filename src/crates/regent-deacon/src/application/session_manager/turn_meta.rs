//! Post-turn usage/cache telemetry accessors. Split from
//! `session_manager/mod.rs` (file-size rule).

use super::*;

impl SessionManager {
    /// The just-finished turn's usage for the status-bar context meter:
    /// `(input_tokens, output_tokens, context_max, cache_read, cache_write)`
    /// where `context_max` is the session's context budget and the two cache
    /// fields (SPL P2) are `Some` only when the provider reported prompt-cache
    /// usage. `None` for an unknown session. Smallest additive accessor so
    /// `prompt.submit` can attach the fields the desktop reads off
    /// `turn.complete` without re-plumbing `run_turn`.
    pub async fn last_turn_usage(
        &self,
        session_id: &SessionId,
    ) -> Option<(u32, u32, u32, Option<u32>, Option<u32>)> {
        let agent_arc = {
            let entries = self.entries.lock().await;
            Arc::clone(&entries.get(session_id)?.agent)
        };
        let agent = agent_arc.lock().await;
        let (input_tokens, output_tokens) = agent.last_turn_usage();
        let (cache_read, cache_write) = agent.last_turn_cache_usage();
        let (_used, context_max) = agent.context_usage();
        Some((
            input_tokens,
            output_tokens,
            context_max,
            cache_read,
            cache_write,
        ))
    }

    /// SPL P2 (§3.1): why the just-finished turn was full-price, when known
    /// (`"compaction"` | `"failover"` | `"routing"` | `"pruning"`). `None` when
    /// no reset happened or the session is unknown — omitted from `turn.complete`
    /// in that case.
    pub async fn last_turn_cache_reset(&self, session_id: &SessionId) -> Option<&'static str> {
        let agent_arc = {
            let entries = self.entries.lock().await;
            Arc::clone(&entries.get(session_id)?.agent)
        };
        let guard = agent_arc.lock().await;
        guard.last_cache_reset()
    }
}

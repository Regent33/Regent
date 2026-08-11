//! Post-turn usage/cache telemetry accessors. Split from
//! `session_manager/mod.rs` (file-size rule).

use super::*;

/// The additive `timings_ms` object on `turn.usage`: where the just-finished
/// turn's wall clock actually went. Lives here rather than inline in `run.rs`
/// because that file is already past the size rule. Surfaces read it as a
/// nested object so new phases can be added without touching the flat fields
/// the desktop and CLI already bind to.
pub(super) fn timings_json(timings: regent_agent::TurnTimings) -> serde_json::Value {
    serde_json::json!({
        "total": timings.total_ms,
        "model": timings.model_ms,
        "tools": timings.tools_ms,
        "store": timings.store_ms,
        "compaction": timings.compact_ms,
        "levers": timings.levers_ms,
    })
}

impl SessionManager {
    /// The just-finished turn's usage for the status-bar context meter:
    /// `(input_tokens, output_tokens, context_max, cache_read, cache_write,
    /// usage_complete, last_request_input_tokens)`
    /// where `context_max` is the session's context budget and the two cache
    /// fields (SPL P2) are `Some` only when the provider reported prompt-cache
    /// usage. `None` for an unknown session. Smallest additive accessor so
    /// `prompt.submit` can attach the fields the desktop reads off
    /// `turn.complete` without re-plumbing `run_turn`.
    pub async fn last_turn_usage(
        &self,
        session_id: &SessionId,
    ) -> Option<(u32, u32, u32, Option<u32>, Option<u32>, bool, Option<u32>)> {
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
            agent.last_turn_usage_complete(),
            agent.last_request_input_tokens(),
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

    /// Fire-and-forget notification onto the deacon's stdio stream — the one
    /// place a background, session-less event can reach the client. Best-effort
    /// by design: a closed channel means nobody is listening, which must never
    /// fail the work that produced the event.
    pub(crate) fn emit_event(&self, method: &str, params: serde_json::Value) {
        let notif = crate::domain::entities::RpcNotification::new(method, params);
        if let Ok(line) = serde_json::to_string(&notif) {
            self.out_tx.send(line).ok();
        }
    }
}

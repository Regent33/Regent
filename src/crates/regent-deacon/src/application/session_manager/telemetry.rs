//! SPL prefix-hash telemetry (§3.3): the per-turn check that keeps the
//! stable-prefix contract honest. Within a session, Tier 0/1 bytes must never
//! change; when they do, the regression is caught here on its first affected
//! turn — as a `cache_bust` warning, never a failed turn (fail-open: this
//! layer has no error path by design).

use super::SessionManager;
use regent_kernel::SessionId;
use std::sync::Arc;

impl SessionManager {
    /// Build-time tier hashes for the additive `turn.complete` fields, after a
    /// fail-open check of what the agent actually sends: its frozen system
    /// prompt string and the freshly re-serialized tool definitions — never
    /// live store reads, which would false-alarm on mid-session persona edits
    /// that don't reach the wire until the next session build. A mismatch logs
    /// a `cache_bust` warning naming the tier and the turn proceeds untouched.
    /// `None` for an unknown session.
    pub async fn turn_prefix_hashes(&self, session_id: &SessionId) -> Option<(String, String)> {
        let (agent_arc, ledger) = {
            let entries = self.entries.lock().await;
            let entry = entries.get(session_id)?;
            (Arc::clone(&entry.agent), Arc::clone(&entry.ledger))
        };
        let agent = agent_arc.lock().await;
        let defs = serde_json::to_string(&agent.tool_definitions()).unwrap_or_default();
        for bust in ledger.check(agent.system_prompt(), &defs) {
            tracing::warn!(
                session = %session_id,
                tier = bust.tier.name(),
                segment = bust.segment,
                "cache_bust: stable-prefix content changed mid-session ({} / {})",
                bust.tier.name(),
                bust.segment,
            );
        }
        Some(ledger.tier_hashes_hex())
    }
}

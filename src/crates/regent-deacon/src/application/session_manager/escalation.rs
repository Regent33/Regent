//! ADR-038 P2: the light→full profile escalation apply path. Split from
//! `build.rs` (file-size rule) — same `SessionManager`, extension impl.

use super::SessionManager;
use crate::domain::errors::DeaconError;
use regent_agent::Agent;
use regent_kernel::SessionId;
use std::sync::{Arc, OnceLock};

impl SessionManager {
    /// Rebuilds this session's prompt + catalog as the FULL profile, swaps
    /// them into the agent (stamping `cache_reset: "profile"`), replaces the
    /// entry's stable-prefix baseline, and persists the new prompt so a later
    /// resume restores full, not the light birth bytes. The caller
    /// (`run_turn`) holds the agent lock; the entries lock is taken only
    /// briefly here and never while awaiting the agent — the same
    /// clone-then-drop discipline every other accessor uses.
    pub(super) async fn escalate_to_full(
        &self,
        session_id: &SessionId,
        agent: &mut Agent,
        conversation_key: Option<&str>,
    ) -> Result<(), DeaconError> {
        let sid_cell = Arc::new(OnceLock::new());
        let _ = sid_cell.set(session_id.to_string());
        let provider = self.provider();
        let (catalog, _review, mut ledger) = self
            .make_catalogs_and_prompt(&provider, &sid_cell, conversation_key, None, false)
            .await?;
        ledger.seal(&serde_json::to_string(&catalog.definitions()).unwrap_or_default());
        let prompt = ledger.render();
        if let Err(e) = self.store.update_session_prompt(session_id, &prompt) {
            tracing::warn!(session = %session_id, error = %e, "escalated prompt persist failed");
        }
        agent.escalate_profile(Arc::new(catalog), prompt);
        if let Some(entry) = self.entries.lock().await.get_mut(session_id) {
            entry.ledger = Arc::new(ledger);
        }
        Ok(())
    }
}

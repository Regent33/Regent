//! Read/query accessors plus the interrupt, approval-resolution, and
//! memory write-approval surface.

use super::SessionManager;
use crate::domain::errors::DeaconError;
use regent_kernel::SessionId;
use regent_store::SessionMeta;
use std::sync::Arc;

impl SessionManager {
    /// SPL §3.4 `context.budget`: the live prompt-composition breakdown for a
    /// session — chars + estimated tokens (chars/4) per Ledger segment, tier
    /// totals (tool definitions ride Tier 0, same as the cache prefix), and
    /// the serialized tool-definitions size. `None` for an unknown session.
    pub async fn context_budget(&self, session_id: &SessionId) -> Option<serde_json::Value> {
        use crate::domain::ledger::Tier;
        use serde_json::json;
        let (ledger, agent_arc) = {
            let entries = self.entries.lock().await;
            let e = entries.get(session_id)?;
            (Arc::clone(&e.ledger), Arc::clone(&e.agent))
        };
        let defs_chars = {
            let agent = agent_arc.lock().await;
            serde_json::to_string(&agent.tool_definitions()).map_or(0, |s| s.len())
        };
        let (mut t0, mut t1) = (defs_chars, 0usize);
        let segments: Vec<_> = ledger
            .segments()
            .iter()
            .map(|s| {
                match s.tier {
                    Tier::Process => t0 += s.text.len(),
                    Tier::Session => t1 += s.text.len(),
                }
                json!({
                    "name": s.name,
                    "tier": s.tier.name(),
                    "chars": s.text.len(),
                    "est_tokens": s.text.len() / 4,
                })
            })
            .collect();
        Some(json!({
            "segments": segments,
            "tool_defs": { "chars": defs_chars, "est_tokens": defs_chars / 4 },
            "tier0": { "chars": t0, "est_tokens": t0 / 4 },
            "tier1": { "chars": t1, "est_tokens": t1 / 4 },
        }))
    }

    pub async fn interrupt(&self, session_id: &SessionId) -> bool {
        let arc = {
            let entries = self.entries.lock().await;
            entries.get(session_id).map(|e| Arc::clone(&e.interrupt))
        };
        if let Some(arc) = arc
            && let Some(token) = arc.lock().await.as_ref()
        {
            token.cancel();
            return true;
        }
        false
    }

    /// `feedback` (additive): deny-reason for a tool gate, or the free-text
    /// answer to an `ask_user` question.
    pub async fn resolve_approval(
        &self,
        session_id: &SessionId,
        approved: bool,
        feedback: Option<String>,
    ) -> bool {
        let arc = {
            let entries = self.entries.lock().await;
            entries
                .get(session_id)
                .map(|e| Arc::clone(&e.approval_pending))
        };
        if let Some(arc) = arc
            && let Some(tx) = arc.lock().await.take()
        {
            return tx.send((approved, feedback)).is_ok();
        }
        false
    }

    pub fn list_sessions(&self, limit: usize) -> Result<Vec<SessionMeta>, DeaconError> {
        self.store.list_sessions(limit).map_err(DeaconError::Store)
    }

    /// The backing store — read access for callers (tests, the tiering
    /// acceptance suite) that need to seed or inspect ledger rows directly.
    #[must_use]
    pub fn store_handle(&self) -> &Arc<regent_store::Store> {
        &self.store
    }

    // ── Session organization (rename/pin/archive/delete) ────────────────────
    // Each returns whether the session row exists (delete: whether it existed).

    pub fn rename_session(&self, id: &SessionId, title: &str) -> Result<bool, DeaconError> {
        self.store
            .rename_session(id, Some(title))
            .map_err(DeaconError::Store)
    }

    pub fn set_session_pinned(&self, id: &SessionId, pinned: bool) -> Result<bool, DeaconError> {
        self.store
            .set_session_pinned(id, pinned)
            .map_err(DeaconError::Store)
    }

    pub fn set_session_archived(
        &self,
        id: &SessionId,
        archived: bool,
    ) -> Result<bool, DeaconError> {
        self.store
            .set_session_archived(id, archived)
            .map_err(DeaconError::Store)
    }

    pub fn delete_session(&self, id: &SessionId) -> Result<bool, DeaconError> {
        self.store.delete_session(id).map_err(DeaconError::Store)
    }

    /// Stored transcript in append order — powers `session.history` (chat
    /// surfaces re-render past messages on resume).
    pub fn session_history(
        &self,
        id: &regent_kernel::SessionId,
    ) -> Result<Vec<regent_store::StoredMessage>, DeaconError> {
        self.store.get_conversation(id).map_err(DeaconError::Store)
    }

    /// Count of sessions currently live in memory (for the `status` surface).
    pub async fn active_sessions(&self) -> usize {
        self.entries.lock().await.len()
    }

    /// Aggregate usage rollup across every session (the `insights` surface).
    pub fn insights(&self) -> Result<regent_store::InsightsRollup, DeaconError> {
        self.store.insights().map_err(DeaconError::Store)
    }

    // ── Persona (DB-backed soul / user profile) ─────────────────────────────
}

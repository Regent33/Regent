//! Read/query accessors plus the interrupt, approval-resolution, and
//! memory write-approval surface.

use super::SessionManager;
use crate::domain::errors::DaemonError;
use regent_kernel::{RegentError, SessionId};
use regent_store::{PendingWriteRow, SearchHit, SessionMeta};
use std::sync::Arc;

impl SessionManager {
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

    pub async fn resolve_approval(&self, session_id: &SessionId, approved: bool) -> bool {
        let arc = {
            let entries = self.entries.lock().await;
            entries.get(session_id).map(|e| Arc::clone(&e.approval_pending))
        };
        if let Some(arc) = arc
            && let Some(tx) = arc.lock().await.take()
        {
            return tx.send(approved).is_ok();
        }
        false
    }

    pub fn list_sessions(&self, limit: usize) -> Result<Vec<SessionMeta>, DaemonError> {
        self.store.list_sessions(limit).map_err(DaemonError::Store)
    }

    /// Count of sessions currently live in memory (for the `status` surface).
    pub async fn active_sessions(&self) -> usize {
        self.entries.lock().await.len()
    }

    /// Aggregate usage rollup across every session (the `insights` surface).
    pub fn insights(&self) -> Result<regent_store::InsightsRollup, DaemonError> {
        self.store.insights().map_err(DaemonError::Store)
    }

    pub fn skills_list(&self) -> Result<Vec<regent_skills::SkillSummary>, DaemonError> {
        self.skills.list().map_err(RegentError::from).map_err(DaemonError::Core)
    }

    pub fn skill_view(&self, name: &str) -> Result<regent_skills::SkillRecord, DaemonError> {
        self.skills.view(name).map_err(RegentError::from).map_err(DaemonError::Core)
    }

    pub fn skill_create(&self, name: &str, description: &str, body: &str) -> Result<(), DaemonError> {
        self.skills
            .create(name, description, body, "user")
            .map_err(RegentError::from)
            .map_err(DaemonError::Core)
    }

    pub fn skill_archive(&self, name: &str) -> Result<(), DaemonError> {
        self.skills.archive(name).map_err(RegentError::from).map_err(DaemonError::Core)
    }

    pub fn search_sessions(&self, query: &str, limit: u32) -> Result<Vec<SearchHit>, DaemonError> {
        self.store.search_messages(query, limit).map_err(DaemonError::Store)
    }

    // ── Model selection ─────────────────────────────────────────────────────

    /// The active model for new sessions (the `model.get` RPC surface).
    #[must_use]
    pub fn model(&self) -> String {
        self.current_model.lock().unwrap().clone()
    }

    /// Switches the model used for **new** sessions. Existing sessions keep
    /// their model so their prompt cache stays valid (a mid-session model
    /// switch would invalidate the whole cached prefix).
    pub fn set_model(&self, model: impl Into<String>) {
        *self.current_model.lock().unwrap() = model.into();
    }

    // ── Memory write-approval (the §10.2 human gate) ────────────────────────

    pub fn pending_memory_writes(&self, limit: u32) -> Result<Vec<PendingWriteRow>, DaemonError> {
        self.graph.pending_writes(limit).map_err(RegentError::from).map_err(DaemonError::Core)
    }

    pub fn approve_memory_write(&self, id: &str) -> Result<Option<String>, DaemonError> {
        self.graph.approve_write(id).map_err(RegentError::from).map_err(DaemonError::Core)
    }

    pub fn reject_memory_write(&self, id: &str) -> Result<bool, DaemonError> {
        self.graph.reject_write(id).map_err(RegentError::from).map_err(DaemonError::Core)
    }

    /// Auto-rejects writes whose approval TTL elapsed; returns how many.
    pub fn expire_memory_writes(&self) -> Result<usize, DaemonError> {
        self.graph
            .expire_pending_writes()
            .map(|expired| expired.len())
            .map_err(RegentError::from)
            .map_err(DaemonError::Core)
    }

    // ── Committed-memory lifecycle (`memory list/pin/unpin/forget`) ─────────

    pub fn list_memory(&self, limit: u32) -> Result<Vec<regent_graph::MemoryNode>, DaemonError> {
        self.graph.recent_nodes(limit).map_err(RegentError::from).map_err(DaemonError::Core)
    }

    pub fn pin_memory(&self, id: &str) -> Result<bool, DaemonError> {
        self.graph.pin(id).map_err(RegentError::from).map_err(DaemonError::Core)
    }

    pub fn unpin_memory(&self, id: &str) -> Result<bool, DaemonError> {
        self.graph.unpin(id).map_err(RegentError::from).map_err(DaemonError::Core)
    }

    pub fn forget_memory(&self, id: &str) -> Result<bool, DaemonError> {
        self.graph.forget(id).map_err(RegentError::from).map_err(DaemonError::Core)
    }
}

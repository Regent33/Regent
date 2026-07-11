//! Read/query accessors plus the interrupt, approval-resolution, and
//! memory write-approval surface.

use super::SessionManager;
use crate::domain::errors::DeaconError;
use regent_kernel::{RegentError, SessionId};
use regent_store::{AgentRow, KanbanTaskRow, PendingWriteRow, SearchHit, SessionMeta};
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
            entries
                .get(session_id)
                .map(|e| Arc::clone(&e.approval_pending))
        };
        if let Some(arc) = arc
            && let Some(tx) = arc.lock().await.take()
        {
            return tx.send(approved).is_ok();
        }
        false
    }

    pub fn list_sessions(&self, limit: usize) -> Result<Vec<SessionMeta>, DeaconError> {
        self.store.list_sessions(limit).map_err(DeaconError::Store)
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

    pub fn persona_get(&self, key: &str) -> Result<String, DeaconError> {
        self.store.get_persona(key).map_err(DeaconError::Store)
    }

    pub fn persona_set(&self, key: &str, content: &str) -> Result<(), DeaconError> {
        self.store
            .set_persona(key, content)
            .map_err(DeaconError::Store)
    }

    // ── Kanban board (the `kanban` CLI surface, on the "default" board) ──────

    /// Adds a task to the default board's `todo` column; returns its id.
    pub fn kanban_create(&self, title: &str, description: &str) -> Result<String, DeaconError> {
        self.store
            .ensure_board("default")
            .map_err(DeaconError::Store)?;
        let id = format!("task_{}", uuid::Uuid::new_v4().simple());
        self.store
            .create_task(&id, "default", title, description)
            .map_err(DeaconError::Store)?;
        Ok(id)
    }

    pub fn kanban_list(&self, status: Option<&str>) -> Result<Vec<KanbanTaskRow>, DeaconError> {
        self.store
            .list_tasks("default", status)
            .map_err(DeaconError::Store)
    }

    pub fn kanban_show(&self, id: &str) -> Result<Option<KanbanTaskRow>, DeaconError> {
        self.store.find_task(id).map_err(DeaconError::Store)
    }

    /// Assigns a `todo` task to `worker` (a named agent), leaving it queued so
    /// the board dispatcher claims and runs it as that agent.
    pub fn kanban_assign(&self, id: &str, worker: &str) -> Result<bool, DeaconError> {
        self.store
            .assign_task(id, worker)
            .map_err(DeaconError::Store)
    }

    /// Moves a task to `status` unconditionally (block/unblock/complete).
    pub fn kanban_set_status(&self, id: &str, status: &str) -> Result<bool, DeaconError> {
        self.store
            .set_task_status(id, status)
            .map_err(DeaconError::Store)
    }

    // ── Named agents ──────────────────────────────────────────────────────────
    pub fn agents_list(&self) -> Result<Vec<AgentRow>, DeaconError> {
        self.store.list_agents().map_err(DeaconError::Store)
    }

    pub fn agents_show(&self, name: &str) -> Result<Option<AgentRow>, DeaconError> {
        self.store.find_agent(name).map_err(DeaconError::Store)
    }

    pub fn agents_set(
        &self,
        name: &str,
        description: &str,
        system_prompt: &str,
        model: Option<&str>,
        tools: Option<&str>,
    ) -> Result<(), DeaconError> {
        self.store
            .upsert_agent(name, description, system_prompt, model, tools)
            .map_err(DeaconError::Store)
    }

    pub fn agents_remove(&self, name: &str) -> Result<bool, DeaconError> {
        self.store.remove_agent(name).map_err(DeaconError::Store)
    }

    pub fn skills_list(&self) -> Result<Vec<regent_skills::SkillSummary>, DeaconError> {
        self.skills
            .list()
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)
    }

    pub fn skills_list_archived(&self) -> Result<Vec<regent_skills::SkillSummary>, DeaconError> {
        self.skills
            .list_archived()
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)
    }

    pub fn skill_view(&self, name: &str) -> Result<regent_skills::SkillRecord, DeaconError> {
        self.skills
            .view(name)
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)
    }

    pub fn skill_create(
        &self,
        name: &str,
        description: &str,
        body: &str,
    ) -> Result<(), DeaconError> {
        self.skills
            .create(name, description, body, "user")
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)
    }

    pub fn skill_archive(&self, name: &str) -> Result<(), DeaconError> {
        self.skills
            .archive(name)
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)
    }

    pub fn skill_unarchive(&self, name: &str) -> Result<(), DeaconError> {
        self.skills
            .unarchive(name)
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)
    }

    pub fn search_sessions(&self, query: &str, limit: u32) -> Result<Vec<SearchHit>, DeaconError> {
        self.store
            .search_messages(query, limit)
            .map_err(DeaconError::Store)
    }

    // ── Model selection ─────────────────────────────────────────────────────

    /// The active model for new sessions (the `model.get` RPC surface).
    #[must_use]
    pub fn model(&self) -> String {
        self.current_model.lock().unwrap().clone()
    }

    /// Switches the active model. New sessions build on it immediately; open
    /// sessions pick it up on their next turn via the routing epoch (the
    /// cached prompt prefix is sacrificed — the user asked to switch).
    pub fn set_model(&self, model: impl Into<String>) {
        *self.current_model.lock().unwrap() = model.into();
        self.bump_routing();
    }

    /// Current routing epoch — sessions stamped below it rebuild their
    /// provider on their next turn.
    pub(super) fn routing_epoch(&self) -> u64 {
        self.routing_epoch
            .load(std::sync::atomic::Ordering::Acquire)
    }

    /// Marks every open session's provider stale (model/key/config changed).
    /// Called by `set_model` and the dispatcher's config/env reload path.
    pub fn bump_routing(&self) {
        self.routing_epoch
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    }

    // ── Memory write-approval (the §10.2 human gate) ────────────────────────

    pub fn pending_memory_writes(&self, limit: u32) -> Result<Vec<PendingWriteRow>, DeaconError> {
        self.graph
            .pending_writes(limit)
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)
    }

    pub fn approve_memory_write(&self, id: &str) -> Result<Option<String>, DeaconError> {
        self.graph
            .approve_write(id)
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)
    }

    pub fn reject_memory_write(&self, id: &str) -> Result<bool, DeaconError> {
        self.graph
            .reject_write(id)
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)
    }

    /// Auto-rejects writes whose approval TTL elapsed; returns how many.
    pub fn expire_memory_writes(&self) -> Result<usize, DeaconError> {
        self.graph
            .expire_pending_writes()
            .map(|expired| expired.len())
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)
    }

    // ── Committed-memory lifecycle (`memory list/pin/unpin/forget`) ─────────

    pub fn list_memory(&self, limit: u32) -> Result<Vec<regent_graph::MemoryNode>, DeaconError> {
        self.graph
            .recent_nodes(limit)
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)
    }

    pub fn pin_memory(&self, id: &str) -> Result<bool, DeaconError> {
        self.graph
            .pin(id)
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)
    }

    pub fn unpin_memory(&self, id: &str) -> Result<bool, DeaconError> {
        self.graph
            .unpin(id)
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)
    }

    pub fn forget_memory(&self, id: &str) -> Result<bool, DeaconError> {
        self.graph
            .forget(id)
            .map_err(RegentError::from)
            .map_err(DeaconError::Core)
    }
}

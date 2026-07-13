//! Workspace directory queries: persona, kanban, agents, skills, session
//! search, and model-routing accessors. Split from `queries.rs` (file-size
//! rule) — same `SessionManager`, extension `impl`.

use super::SessionManager;
use crate::domain::errors::DeaconError;
use regent_kernel::RegentError;
use regent_store::{AgentRow, KanbanTaskRow, SearchHit};

impl SessionManager {
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
    /// cached prompt prefix is sacrificed — the user asked to switch). Emits
    /// a `model.changed` notification so every surface showing the active
    /// model (composer pill, status bar) updates without re-probing.
    pub fn set_model(&self, model: impl Into<String>) {
        let model = model.into();
        *self.current_model.lock().unwrap() = model.clone();
        self.bump_routing();
        let notification = crate::domain::entities::RpcNotification::new(
            "model.changed",
            serde_json::json!({"model": model}),
        );
        if let Ok(line) = serde_json::to_string(&notification) {
            self.out_tx.send(line).ok();
        }
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
}

//! The dispatch loop: claim the next `todo` task, run it, and resolve it by
//! the board's review policy (`human` waits · `auto` self-approves · `agent`
//! runs the reviewer). The deacon owns the tick loop around `dispatch_once`.

use super::{ReviewVerdict, Reviewer, TaskRunner};
use regent_kernel::RegentError;
use regent_store::{KanbanTaskRow, ReviewPolicy, Store, StoreError};
use std::sync::Arc;

/// What a dispatch pass did to one task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskOutcome {
    pub id: String,
    /// `in_review` (awaiting human) | `done` (auto/approved) | `in_progress`
    /// (review rejected → rework) | `blocked` (run failed).
    pub status: String,
    pub summary: String,
}

pub struct BoardDispatcher {
    store: Arc<Store>,
    runner: Arc<dyn TaskRunner>,
    reviewer: Option<Arc<dyn Reviewer>>,
    worker_id: String,
}

impl BoardDispatcher {
    #[must_use]
    pub fn new(
        store: Arc<Store>,
        runner: Arc<dyn TaskRunner>,
        worker_id: impl Into<String>,
    ) -> Self {
        Self {
            store,
            runner,
            reviewer: None,
            worker_id: worker_id.into(),
        }
    }

    /// Attaches the reviewer used by boards with the `agent` review policy.
    /// Without one, `agent` boards fall back to `human` review (fail-safe).
    #[must_use]
    pub fn with_reviewer(mut self, reviewer: Arc<dyn Reviewer>) -> Self {
        self.reviewer = Some(reviewer);
        self
    }

    /// Claims and runs the next `todo` task on `board`. `None` when there's
    /// nothing claimable (empty, or another worker won the race).
    pub async fn dispatch_once(&self, board: &str) -> Result<Option<TaskOutcome>, RegentError> {
        let todos = self
            .store
            .list_tasks(board, Some("todo"))
            .map_err(store_err)?;
        let Some(task) = todos.into_iter().next() else {
            return Ok(None);
        };

        // Atomic claim — the race guard. If we lost it, leave it for the winner.
        if !self
            .store
            .claim_task(&task.id, &self.worker_id)
            .map_err(store_err)?
        {
            return Ok(None);
        }

        let (status, summary) = match self.runner.run(&task).await {
            // Clean run → resolve by the board's review policy.
            Ok(summary) => self.resolve_review(board, &task, summary).await?,
            // Failure auto-blocks (valid from any column) for inspection.
            Err(error) => {
                self.store
                    .set_task_status(&task.id, "blocked")
                    .map_err(store_err)?;
                ("blocked", error.to_string())
            }
        };
        Ok(Some(TaskOutcome {
            id: task.id,
            status: status.to_owned(),
            summary,
        }))
    }

    /// Drains up to `max` claimable tasks this tick — the deacon's per-tick
    /// budget, so one busy board can't starve the runtime. Stops early when
    /// the board runs dry. Each task's failure is captured as its outcome, so
    /// one bad task never aborts the drain.
    pub async fn dispatch_pending(
        &self,
        board: &str,
        max: usize,
    ) -> Result<Vec<TaskOutcome>, RegentError> {
        let mut outcomes = Vec::new();
        for _ in 0..max {
            match self.dispatch_once(board).await? {
                Some(outcome) => outcomes.push(outcome),
                None => break,
            }
        }
        Ok(outcomes)
    }

    /// Lands the task in `in_review` (always — for an audit trail), then
    /// resolves it by the board's policy: `human` waits, `auto` self-approves,
    /// `agent` runs the reviewer.
    async fn resolve_review(
        &self,
        board: &str,
        task: &KanbanTaskRow,
        summary: String,
    ) -> Result<(&'static str, String), RegentError> {
        self.store
            .transition_task(&task.id, "in_progress", "in_review")
            .map_err(store_err)?;
        match self.store.board_policy(board).map_err(store_err)? {
            ReviewPolicy::Human => Ok(("in_review", summary)),
            ReviewPolicy::Auto => {
                self.store
                    .transition_task(&task.id, "in_review", "done")
                    .map_err(store_err)?;
                Ok(("done", summary))
            }
            ReviewPolicy::Agent => self.agent_review(task, summary).await,
        }
    }

    async fn agent_review(
        &self,
        task: &KanbanTaskRow,
        summary: String,
    ) -> Result<(&'static str, String), RegentError> {
        let Some(reviewer) = &self.reviewer else {
            // Policy says `agent` but none is wired → hold for a human.
            return Ok(("in_review", summary));
        };
        match reviewer.review(task, &summary).await? {
            ReviewVerdict::Approve => {
                self.store
                    .transition_task(&task.id, "in_review", "done")
                    .map_err(store_err)?;
                Ok(("done", summary))
            }
            // Reject is feedback, not a dead end: park back in `in_progress`.
            // It is NOT auto-re-dispatched (dispatch claims only `todo`), so a
            // bad reviewer can't trigger a retry storm — re-queue is deliberate.
            ReviewVerdict::Reject(reason) => {
                self.store
                    .transition_task(&task.id, "in_review", "in_progress")
                    .map_err(store_err)?;
                Ok((
                    "in_progress",
                    format!("review rejected: {reason}\n\n{summary}"),
                ))
            }
        }
    }
}

fn store_err(error: StoreError) -> RegentError {
    RegentError::Store(error.to_string())
}

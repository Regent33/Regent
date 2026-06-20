//! Kanban board persistence — the shared work board for multi-agent
//! orchestration. Dumb CRUD with one critical invariant: claiming is **atomic**
//! (a single conditional UPDATE) so two workers never grab the same task. All
//! board *policy* (who dispatches, failure auto-block) lives above this layer.

use crate::domain::entities::KanbanTaskRow;
use crate::domain::errors::StoreError;
use crate::infra::db::{Store, now_epoch};
use rusqlite::{OptionalExtension, params};

const COLUMNS: &str = "id, board, title, description, status, assignee, created_at, updated_at";

fn row_to_task(row: &rusqlite::Row<'_>) -> Result<KanbanTaskRow, rusqlite::Error> {
    Ok(KanbanTaskRow {
        id: row.get(0)?,
        board: row.get(1)?,
        title: row.get(2)?,
        description: row.get(3)?,
        status: row.get(4)?,
        assignee: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

impl Store {
    /// Adds a task in the `todo` column.
    pub fn create_task(
        &self,
        id: &str,
        board: &str,
        title: &str,
        description: &str,
    ) -> Result<(), StoreError> {
        self.with_write(|tx| {
            let now = now_epoch();
            tx.execute(
                "INSERT INTO kanban_tasks
                 (id, board, title, description, status, assignee, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 'todo', NULL, ?5, ?5)",
                params![id, board, title, description, now],
            )?;
            Ok(())
        })
    }

    /// Tasks on a board, optionally filtered by status, oldest first.
    pub fn list_tasks(
        &self,
        board: &str,
        status: Option<&str>,
    ) -> Result<Vec<KanbanTaskRow>, StoreError> {
        self.with_read(|conn| match status {
            Some(status) => {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {COLUMNS} FROM kanban_tasks WHERE board = ?1 AND status = ?2
                     ORDER BY created_at, id"
                ))?;
                stmt.query_map(params![board, status], row_to_task)?.collect()
            }
            None => {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {COLUMNS} FROM kanban_tasks WHERE board = ?1 ORDER BY created_at, id"
                ))?;
                stmt.query_map(params![board], row_to_task)?.collect()
            }
        })
    }

    pub fn find_task(&self, id: &str) -> Result<Option<KanbanTaskRow>, StoreError> {
        self.with_read(|conn| {
            conn.query_row(
                &format!("SELECT {COLUMNS} FROM kanban_tasks WHERE id = ?1"),
                params![id],
                row_to_task,
            )
            .optional()
        })
    }

    /// Atomically claims a `todo` task for `assignee` (→ `in_progress`).
    /// Returns false if the task was already claimed/gone — the race-free guard
    /// that stops two workers taking the same task.
    pub fn claim_task(&self, id: &str, assignee: &str) -> Result<bool, StoreError> {
        self.with_write(|tx| {
            let changed = tx.execute(
                "UPDATE kanban_tasks SET status = 'in_progress', assignee = ?1, updated_at = ?2
                 WHERE id = ?3 AND status = 'todo'",
                params![assignee, now_epoch(), id],
            )?;
            Ok(changed > 0)
        })
    }

    /// Moves a task to a new status unconditionally. Returns false if the task
    /// doesn't exist. Used for `block`, which is valid from any column.
    pub fn set_task_status(&self, id: &str, status: &str) -> Result<bool, StoreError> {
        self.with_write(|tx| {
            let changed = tx.execute(
                "UPDATE kanban_tasks SET status = ?1, updated_at = ?2 WHERE id = ?3",
                params![status, now_epoch(), id],
            )?;
            Ok(changed > 0)
        })
    }

    /// Atomically moves a task `from` → `to` only if it's currently in `from`.
    /// Returns false when the precondition doesn't hold (wrong column, or the
    /// task is gone) — enforces the workflow (e.g. approve only from review).
    pub fn transition_task(&self, id: &str, from: &str, to: &str) -> Result<bool, StoreError> {
        self.with_write(|tx| {
            let changed = tx.execute(
                "UPDATE kanban_tasks SET status = ?1, updated_at = ?2 WHERE id = ?3 AND status = ?4",
                params![to, now_epoch(), id, from],
            )?;
            Ok(changed > 0)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_list_by_board_and_status() {
        let store = Store::open_in_memory().unwrap();
        store.create_task("t1", "alpha", "first", "").unwrap();
        store.create_task("t2", "alpha", "second", "").unwrap();
        store.create_task("t3", "beta", "other board", "").unwrap();

        assert_eq!(store.list_tasks("alpha", None).unwrap().len(), 2);
        assert_eq!(store.list_tasks("alpha", Some("todo")).unwrap().len(), 2);
        assert_eq!(store.list_tasks("beta", None).unwrap().len(), 1, "boards are isolated");
    }

    #[test]
    fn claim_is_atomic_and_single_winner() {
        let store = Store::open_in_memory().unwrap();
        store.create_task("t1", "alpha", "task", "").unwrap();

        assert!(store.claim_task("t1", "worker-a").unwrap(), "first claim wins");
        assert!(!store.claim_task("t1", "worker-b").unwrap(), "second claim loses the race");

        let task = store.find_task("t1").unwrap().unwrap();
        assert_eq!(task.status, "in_progress");
        assert_eq!(task.assignee.as_deref(), Some("worker-a"));
    }

    #[test]
    fn status_moves_and_filters() {
        let store = Store::open_in_memory().unwrap();
        store.create_task("t1", "alpha", "task", "").unwrap();
        store.claim_task("t1", "w").unwrap();
        assert!(store.set_task_status("t1", "done").unwrap());

        assert!(store.list_tasks("alpha", Some("todo")).unwrap().is_empty());
        assert_eq!(store.list_tasks("alpha", Some("done")).unwrap().len(), 1);
        assert!(!store.set_task_status("nope", "done").unwrap(), "missing task → false");
    }

    #[test]
    fn transition_enforces_the_from_column() {
        let store = Store::open_in_memory().unwrap();
        store.create_task("t1", "alpha", "task", "").unwrap();
        store.claim_task("t1", "w").unwrap(); // → in_progress

        // worker submits for review, reviewer approves
        assert!(store.transition_task("t1", "in_progress", "in_review").unwrap());
        assert!(store.transition_task("t1", "in_review", "done").unwrap());
        // approving again (no longer in_review) is rejected
        assert!(!store.transition_task("t1", "in_review", "done").unwrap());
        // a transition whose `from` doesn't match is a no-op
        assert!(!store.transition_task("t1", "todo", "in_progress").unwrap());
    }
}

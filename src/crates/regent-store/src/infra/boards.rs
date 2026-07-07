//! Board configuration — per-board review policy. A board with no row defaults
//! to `human` review (the fail-safe: work is never silently auto-completed).
//! Board *policy* is read by the dispatcher above this layer; here it's CRUD.

use crate::domain::entities::{BoardRow, ReviewPolicy};
use crate::domain::errors::StoreError;
use crate::infra::db::{Store, now_epoch};
use rusqlite::{OptionalExtension, params};

impl Store {
    /// Creates a board's config row if it doesn't exist (idempotent), leaving
    /// the default `human` policy. Call before setting a policy.
    pub fn ensure_board(&self, board: &str) -> Result<(), StoreError> {
        self.with_write(|tx| {
            tx.execute(
                "INSERT OR IGNORE INTO boards (board, review_policy, reviewer_agent, created_at)
                 VALUES (?1, 'human', NULL, ?2)",
                params![board, now_epoch()],
            )?;
            Ok(())
        })
    }

    /// Sets a board's review policy (upserting the row). `reviewer_agent` names
    /// the profile used when `policy` is `Agent`; ignored otherwise.
    pub fn set_board_policy(
        &self,
        board: &str,
        policy: ReviewPolicy,
        reviewer_agent: Option<&str>,
    ) -> Result<(), StoreError> {
        self.with_write(|tx| {
            tx.execute(
                "INSERT INTO boards (board, review_policy, reviewer_agent, created_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(board) DO UPDATE SET
                     review_policy = excluded.review_policy,
                     reviewer_agent = excluded.reviewer_agent",
                params![board, policy.as_str(), reviewer_agent, now_epoch()],
            )?;
            Ok(())
        })
    }

    /// The board's full config row, if it has one.
    pub fn find_board(&self, board: &str) -> Result<Option<BoardRow>, StoreError> {
        self.with_read(|conn| {
            conn.query_row(
                "SELECT board, review_policy, reviewer_agent, created_at FROM boards WHERE board = ?1",
                params![board],
                |row| {
                    Ok(BoardRow {
                        board: row.get(0)?,
                        review_policy: ReviewPolicy::parse(&row.get::<_, String>(1)?),
                        reviewer_agent: row.get(2)?,
                        created_at: row.get(3)?,
                    })
                },
            )
            .optional()
        })
    }

    /// The board's review policy, defaulting to `human` when unconfigured —
    /// the fail-safe the dispatcher relies on.
    pub fn board_policy(&self, board: &str) -> Result<ReviewPolicy, StoreError> {
        Ok(self
            .find_board(board)?
            .map_or(ReviewPolicy::Human, |b| b.review_policy))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unconfigured_board_defaults_to_human() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(
            store.board_policy("never-seen").unwrap(),
            ReviewPolicy::Human
        );
        assert!(store.find_board("never-seen").unwrap().is_none());
    }

    #[test]
    fn set_and_read_back_policy() {
        let store = Store::open_in_memory().unwrap();
        store
            .set_board_policy("alpha", ReviewPolicy::Agent, Some("reviewer-bot"))
            .unwrap();

        assert_eq!(store.board_policy("alpha").unwrap(), ReviewPolicy::Agent);
        let row = store.find_board("alpha").unwrap().unwrap();
        assert_eq!(row.reviewer_agent.as_deref(), Some("reviewer-bot"));
    }

    #[test]
    fn set_policy_upserts_and_clears_reviewer() {
        let store = Store::open_in_memory().unwrap();
        store
            .set_board_policy("alpha", ReviewPolicy::Agent, Some("bot"))
            .unwrap();
        // Switching to auto overwrites the row and drops the reviewer.
        store
            .set_board_policy("alpha", ReviewPolicy::Auto, None)
            .unwrap();

        assert_eq!(store.board_policy("alpha").unwrap(), ReviewPolicy::Auto);
        assert!(
            store
                .find_board("alpha")
                .unwrap()
                .unwrap()
                .reviewer_agent
                .is_none()
        );
    }

    #[test]
    fn ensure_board_is_idempotent_and_keeps_human_default() {
        let store = Store::open_in_memory().unwrap();
        store.ensure_board("alpha").unwrap();
        store.ensure_board("alpha").unwrap(); // no-op second time
        assert_eq!(store.board_policy("alpha").unwrap(), ReviewPolicy::Human);
    }
}

//! Conversation→session binding. A platform conversation key (`slack:C123`,
//! `discord:456`, …) maps to one Regent session so a chat surface keeps a
//! continuous session across messages. Dumb CRUD; the "find-or-create" policy
//! lives in the deacon session manager above this layer.

use crate::domain::errors::StoreError;
use crate::infra::db::{Store, now_epoch};
use rusqlite::{OptionalExtension, params};

impl Store {
    /// Binds `conversation_key` to `session_id` (last write wins).
    pub fn bind_conversation(
        &self,
        conversation_key: &str,
        session_id: &str,
    ) -> Result<(), StoreError> {
        self.with_write(|tx| {
            tx.execute(
                "INSERT INTO conversation_sessions (conversation_key, session_id, created_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(conversation_key) DO UPDATE SET session_id = excluded.session_id",
                params![conversation_key, session_id, now_epoch()],
            )?;
            Ok(())
        })
    }

    /// The session bound to `conversation_key`, if any.
    pub fn conversation_session(
        &self,
        conversation_key: &str,
    ) -> Result<Option<String>, StoreError> {
        self.with_read(|conn| {
            conn.query_row(
                "SELECT session_id FROM conversation_sessions WHERE conversation_key = ?1",
                params![conversation_key],
                |row| row.get(0),
            )
            .optional()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_lookup_and_rebind() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.conversation_session("slack:C1").unwrap(), None);

        store.bind_conversation("slack:C1", "sess-a").unwrap();
        assert_eq!(
            store.conversation_session("slack:C1").unwrap().as_deref(),
            Some("sess-a")
        );

        // Re-binding the same key replaces (a session was reset/recreated).
        store.bind_conversation("slack:C1", "sess-b").unwrap();
        assert_eq!(
            store.conversation_session("slack:C1").unwrap().as_deref(),
            Some("sess-b")
        );

        // Distinct keys are independent.
        store.bind_conversation("discord:9", "sess-c").unwrap();
        assert_eq!(
            store.conversation_session("discord:9").unwrap().as_deref(),
            Some("sess-c")
        );
        assert_eq!(
            store.conversation_session("slack:C1").unwrap().as_deref(),
            Some("sess-b")
        );
    }
}

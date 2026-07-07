use crate::domain::entities::{SessionMeta, TurnRecord};
use crate::domain::errors::StoreError;
use crate::infra::db::Store;
use regent_kernel::SessionId;
use rusqlite::{OptionalExtension, params};

// title/pinned/archived are appended last so the leading indices match every
// pre-organization read site (additive change, no reindexing).
const SESSION_COLUMNS: &str = "id, source, model, system_prompt, parent_session_id, started_at, \
     ended_at, end_reason, message_count, input_tokens, output_tokens, api_call_count, \
     title, pinned, archived";

fn row_to_meta(row: &rusqlite::Row<'_>) -> Result<SessionMeta, rusqlite::Error> {
    Ok(SessionMeta {
        id: row.get(0)?,
        source: row.get(1)?,
        model: row.get(2)?,
        system_prompt: row.get(3)?,
        parent_session_id: row.get(4)?,
        started_at: row.get(5)?,
        ended_at: row.get(6)?,
        end_reason: row.get(7)?,
        message_count: row.get(8)?,
        input_tokens: row.get(9)?,
        output_tokens: row.get(10)?,
        api_call_count: row.get(11)?,
        title: row.get(12)?,
        pinned: row.get(13)?,
        archived: row.get(14)?,
    })
}

impl Store {
    pub fn session_meta(&self, id: &SessionId) -> Result<SessionMeta, StoreError> {
        let meta = self.with_read(|conn| {
            conn.query_row(
                &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"),
                params![id.as_str()],
                row_to_meta,
            )
            .optional()
        })?;
        meta.ok_or_else(|| StoreError::UnknownSession(id.to_string()))
    }

    /// Returns the most recent sessions ordered by start time (newest first).
    /// Archived sessions are included — the surface filters them; the query
    /// stays lazy so nothing that used to appear silently drops out.
    pub fn list_sessions(&self, limit: usize) -> Result<Vec<SessionMeta>, StoreError> {
        self.with_read(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {SESSION_COLUMNS} FROM sessions ORDER BY started_at DESC LIMIT ?1"
            ))?;
            let rows = stmt.query_map(params![limit as i64], row_to_meta)?;
            rows.collect()
        })
    }

    /// Sets (or clears) a session's human title. Returns whether the row exists.
    pub fn rename_session(&self, id: &SessionId, title: Option<&str>) -> Result<bool, StoreError> {
        self.with_write(|tx| {
            Ok(tx.execute(
                "UPDATE sessions SET title = ?1 WHERE id = ?2",
                params![title, id.as_str()],
            )? > 0)
        })
    }

    /// Pins/unpins a session. Returns whether the row exists.
    pub fn set_session_pinned(&self, id: &SessionId, pinned: bool) -> Result<bool, StoreError> {
        self.with_write(|tx| {
            Ok(tx.execute(
                "UPDATE sessions SET pinned = ?1 WHERE id = ?2",
                params![pinned, id.as_str()],
            )? > 0)
        })
    }

    /// Archives/unarchives a session. Returns whether the row exists.
    pub fn set_session_archived(&self, id: &SessionId, archived: bool) -> Result<bool, StoreError> {
        self.with_write(|tx| {
            Ok(tx.execute(
                "UPDATE sessions SET archived = ?1 WHERE id = ?2",
                params![archived, id.as_str()],
            )? > 0)
        })
    }

    /// Permanently removes a session and all its history (messages + turns).
    /// `messages_fts` is cleaned by the AFTER DELETE trigger on `messages`.
    /// Returns whether the session row existed.
    pub fn delete_session(&self, id: &SessionId) -> Result<bool, StoreError> {
        self.with_write(|tx| {
            tx.execute(
                "DELETE FROM messages WHERE session_id = ?1",
                params![id.as_str()],
            )?;
            tx.execute(
                "DELETE FROM turns WHERE session_id = ?1",
                params![id.as_str()],
            )?;
            let removed = tx.execute("DELETE FROM sessions WHERE id = ?1", params![id.as_str()])?;
            Ok(removed > 0)
        })
    }

    pub fn turns_for_session(&self, id: &SessionId) -> Result<Vec<TurnRecord>, StoreError> {
        self.with_read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT model, api_calls, outcome, error, started_at, ended_at
                 FROM turns WHERE session_id = ?1 ORDER BY id",
            )?;
            let rows = stmt.query_map(params![id.as_str()], |row| {
                Ok(TurnRecord {
                    model: row.get(0)?,
                    api_calls: row.get(1)?,
                    outcome: row.get(2)?,
                    error: row.get(3)?,
                    started_at: row.get(4)?,
                    ended_at: row.get(5)?,
                })
            })?;
            rows.collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::Store;
    use regent_kernel::SessionId;

    fn seed(store: &Store, id: &str) -> SessionId {
        let sid = SessionId::from_string(id);
        store.create_session(&sid, "cli", None, None, None).unwrap();
        sid
    }

    #[test]
    fn rename_pin_archive_roundtrip_through_list() {
        let store = Store::open_in_memory().unwrap();
        let sid = seed(&store, "sess-1");

        // Defaults: no title, not pinned, not archived.
        let meta = store.session_meta(&sid).unwrap();
        assert_eq!(meta.title, None);
        assert!(!meta.pinned && !meta.archived);

        assert!(store.rename_session(&sid, Some("My chat")).unwrap());
        assert!(store.set_session_pinned(&sid, true).unwrap());
        assert!(store.set_session_archived(&sid, true).unwrap());

        // Roundtrip through list_sessions (the RPC read path).
        let listed = store.list_sessions(20).unwrap();
        let row = listed.iter().find(|m| m.id == "sess-1").unwrap();
        assert_eq!(row.title.as_deref(), Some("My chat"));
        assert!(row.pinned && row.archived, "archived rows still list");

        // Unknown session → false, no error.
        assert!(
            !store
                .rename_session(&SessionId::from_string("nope"), Some("x"))
                .unwrap()
        );
    }

    #[test]
    fn delete_removes_session_and_history() {
        let store = Store::open_in_memory().unwrap();
        let sid = seed(&store, "sess-del");
        store
            .append_message(&sid, &regent_kernel::ChatMessage::user("hello"), None, None)
            .unwrap();

        assert!(store.delete_session(&sid).unwrap());
        assert!(store.session_meta(&sid).is_err(), "session row gone");
        assert!(store.get_conversation(&sid).is_err(), "history gone");
        // Deleting again is a no-op (already removed).
        assert!(!store.delete_session(&sid).unwrap());
    }
}

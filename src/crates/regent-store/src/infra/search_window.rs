//! The turns surrounding a search hit (W11). Split from `search.rs`
//! (file-size rule).

use crate::domain::entities::WindowMessage;
use crate::domain::errors::StoreError;
use crate::infra::db::Store;
use rusqlite::params;

impl Store {
    /// The `radius` messages either side of `message_id`, in conversation
    /// order, nearest-first-out (offsets `-radius..=-1` then `1..=radius`).
    ///
    /// Ordered by `id`, not `timestamp`: `id` is `AUTOINCREMENT` and therefore
    /// *is* append order, while two messages written inside the same tick can
    /// tie on `timestamp` and come back reversed. The hit itself is excluded —
    /// the caller already has it.
    ///
    /// ponytail: two small `LIMIT`ed scans rather than one window function.
    /// `idx_messages_session` covers the `session_id` filter and the residual
    /// ordering is over one session's rows; if a session ever grows large
    /// enough for that to matter, add `(session_id, id)`.
    pub fn message_window(
        &self,
        session_id: &str,
        message_id: i64,
        radius: u32,
    ) -> Result<Vec<WindowMessage>, StoreError> {
        if radius == 0 {
            return Ok(Vec::new());
        }
        self.with_read(|conn| {
            let mut out = Vec::new();
            // Before: walk backwards from the hit, then flip so the result
            // reads forwards like the conversation did.
            let mut stmt = conn.prepare(
                "SELECT role, content FROM messages
                 WHERE session_id = ?1 AND id < ?2 AND content IS NOT NULL
                 ORDER BY id DESC LIMIT ?3",
            )?;
            let mut before: Vec<WindowMessage> = stmt
                .query_map(params![session_id, message_id, radius], |row| {
                    Ok(WindowMessage {
                        role: row.get(0)?,
                        content: row.get(1)?,
                        offset: 0,
                    })
                })?
                .collect::<Result<_, _>>()?;
            for (i, message) in before.iter_mut().enumerate() {
                message.offset = -(i as i64 + 1);
            }
            before.reverse();
            out.extend(before);

            let mut stmt = conn.prepare(
                "SELECT role, content FROM messages
                 WHERE session_id = ?1 AND id > ?2 AND content IS NOT NULL
                 ORDER BY id ASC LIMIT ?3",
            )?;
            let after = stmt.query_map(params![session_id, message_id, radius], |row| {
                Ok(WindowMessage {
                    role: row.get(0)?,
                    content: row.get(1)?,
                    offset: 0,
                })
            })?;
            for (i, message) in after.enumerate() {
                let mut message = message?;
                message.offset = i as i64 + 1;
                out.push(message);
            }
            Ok(out)
        })
    }
}

#[cfg(test)]
#[path = "search_window_tests.rs"]
mod tests;

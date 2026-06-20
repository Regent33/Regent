//! Write-approval staging persistence — the queue of long-term memory writes
//! awaiting human approval. Dumb CRUD; the approve/reject *policy* lives in
//! `regent-graph`.

use crate::domain::entities::PendingWriteRow;
use crate::domain::errors::StoreError;
use crate::infra::db::Store;
use rusqlite::{OptionalExtension, params};

const COLUMNS: &str =
    "id, kind, name, content, provenance, trust, session_id, ttl_secs, created_at";

fn row_to_pending(row: &rusqlite::Row<'_>) -> Result<PendingWriteRow, rusqlite::Error> {
    Ok(PendingWriteRow {
        id: row.get(0)?,
        kind: row.get(1)?,
        name: row.get(2)?,
        content: row.get(3)?,
        provenance: row.get(4)?,
        trust: row.get(5)?,
        session_id: row.get(6)?,
        ttl_secs: row.get(7)?,
        created_at: row.get(8)?,
    })
}

impl Store {
    /// Stages a proposed memory write for later approval.
    pub fn enqueue_pending_write(&self, write: &PendingWriteRow) -> Result<(), StoreError> {
        self.with_write(|tx| {
            tx.execute(
                "INSERT INTO pending_memory_writes
                 (id, kind, name, content, provenance, trust, session_id, ttl_secs, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    write.id, write.kind, write.name, write.content, write.provenance,
                    write.trust, write.session_id, write.ttl_secs, write.created_at,
                ],
            )?;
            Ok(())
        })
    }

    /// Lists staged writes, oldest first.
    pub fn list_pending_writes(&self, limit: u32) -> Result<Vec<PendingWriteRow>, StoreError> {
        self.with_read(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {COLUMNS} FROM pending_memory_writes ORDER BY created_at, id LIMIT ?1"
            ))?;
            stmt.query_map(params![limit], row_to_pending)?.collect()
        })
    }

    /// Atomically reads and removes one staged write — the consume step shared
    /// by approve (commit it) and reject (discard it). `None` if already gone.
    pub fn take_pending_write(&self, id: &str) -> Result<Option<PendingWriteRow>, StoreError> {
        self.with_write(|tx| {
            let row = tx
                .query_row(
                    &format!("SELECT {COLUMNS} FROM pending_memory_writes WHERE id = ?1"),
                    params![id],
                    row_to_pending,
                )
                .optional()?;
            if row.is_some() {
                tx.execute("DELETE FROM pending_memory_writes WHERE id = ?1", params![id])?;
            }
            Ok(row)
        })
    }

    /// Removes staged writes whose per-row TTL has elapsed (`created_at +
    /// ttl_secs < now`); rows with no TTL never auto-expire. Returns the
    /// removed rows so the caller can log them as auto-rejected.
    pub fn delete_expired_pending_writes(
        &self,
        now: f64,
    ) -> Result<Vec<PendingWriteRow>, StoreError> {
        self.with_write(|tx| {
            let expired: Vec<PendingWriteRow> = {
                let mut stmt = tx.prepare(&format!(
                    "SELECT {COLUMNS} FROM pending_memory_writes
                     WHERE ttl_secs IS NOT NULL AND created_at + ttl_secs < ?1"
                ))?;
                stmt.query_map(params![now], row_to_pending)?.collect::<Result<_, _>>()?
            };
            for write in &expired {
                tx.execute("DELETE FROM pending_memory_writes WHERE id = ?1", params![write.id])?;
            }
            Ok(expired)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(id: &str, ttl: Option<f64>, created: f64) -> PendingWriteRow {
        PendingWriteRow {
            id: id.to_owned(),
            kind: "fact".to_owned(),
            name: id.to_owned(),
            content: format!("content {id}"),
            provenance: "agent_inferred".to_owned(),
            trust: 0.7,
            session_id: Some("sess_1".to_owned()),
            ttl_secs: ttl,
            created_at: created,
        }
    }

    #[test]
    fn enqueue_and_list_oldest_first() {
        let store = Store::open_in_memory().unwrap();
        store.enqueue_pending_write(&write("b", None, 20.0)).unwrap();
        store.enqueue_pending_write(&write("a", None, 10.0)).unwrap();
        let pending = store.list_pending_writes(10).unwrap();
        assert_eq!(pending.iter().map(|w| w.id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
    }

    #[test]
    fn take_returns_then_removes() {
        let store = Store::open_in_memory().unwrap();
        store.enqueue_pending_write(&write("a", None, 1.0)).unwrap();
        let taken = store.take_pending_write("a").unwrap().unwrap();
        assert_eq!(taken.content, "content a");
        assert!(store.take_pending_write("a").unwrap().is_none(), "second take is empty");
        assert!(store.list_pending_writes(10).unwrap().is_empty());
    }

    #[test]
    fn expiry_removes_only_elapsed_ttl_rows() {
        let store = Store::open_in_memory().unwrap();
        // Distinct created_at so list order is unambiguous (time, then id).
        store.enqueue_pending_write(&write("old", Some(5.0), 0.0)).unwrap(); // expires at 5
        store.enqueue_pending_write(&write("fresh", Some(100.0), 1.0)).unwrap(); // expires at 101
        store.enqueue_pending_write(&write("forever", None, 2.0)).unwrap(); // never

        let removed = store.delete_expired_pending_writes(10.0).unwrap();
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].id, "old");
        let remaining: Vec<String> =
            store.list_pending_writes(10).unwrap().into_iter().map(|w| w.id).collect();
        assert_eq!(remaining, ["fresh", "forever"]);
    }
}

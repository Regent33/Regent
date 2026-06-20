use crate::domain::entities::{SessionMeta, TurnRecord};
use crate::domain::errors::StoreError;
use crate::infra::db::Store;
use regent_kernel::SessionId;
use rusqlite::{OptionalExtension, params};

impl Store {
    pub fn session_meta(&self, id: &SessionId) -> Result<SessionMeta, StoreError> {
        let meta = self.with_read(|conn| {
            conn.query_row(
                "SELECT id, source, model, system_prompt, parent_session_id, started_at,
                        ended_at, end_reason, message_count, input_tokens, output_tokens,
                        api_call_count
                 FROM sessions WHERE id = ?1",
                params![id.as_str()],
                |row| {
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
                    })
                },
            )
            .optional()
        })?;
        meta.ok_or_else(|| StoreError::UnknownSession(id.to_string()))
    }

    /// Returns the most recent sessions ordered by start time (newest first).
    pub fn list_sessions(&self, limit: usize) -> Result<Vec<SessionMeta>, StoreError> {
        self.with_read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, source, model, system_prompt, parent_session_id, started_at,
                        ended_at, end_reason, message_count, input_tokens, output_tokens,
                        api_call_count
                 FROM sessions ORDER BY started_at DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit as i64], |row| {
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
                })
            })?;
            rows.collect()
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

//! Message/usage persistence for sessions. Split from `sessions.rs`
//! (file-size rule) — extension impl on the same Store.

use super::sessions::row_to_stored;
use crate::domain::entities::StoredMessage;
use crate::domain::errors::StoreError;
use crate::infra::db::{Store, now_epoch};
use regent_kernel::{ChatMessage, SessionId};
use rusqlite::{OptionalExtension, params};

impl Store {
    /// Appends one message and bumps the session counter. Returns the row id.
    pub fn append_message(
        &self,
        session_id: &SessionId,
        message: &ChatMessage,
        token_count: Option<i64>,
        finish_reason: Option<&str>,
    ) -> Result<i64, StoreError> {
        let tool_calls_json =
            if message.tool_calls.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&message.tool_calls).map_err(|e| {
                    StoreError::CorruptRow(format!("tool_calls serialization: {e}"))
                })?)
            };
        self.with_write(|tx| {
            tx.execute(
                "INSERT INTO messages (session_id, role, content, tool_call_id, tool_calls,
                                       tool_name, reasoning, timestamp, token_count, finish_reason)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    session_id.as_str(),
                    message.role.as_str(),
                    message.content,
                    message.tool_call_id,
                    tool_calls_json,
                    message.tool_name,
                    message.reasoning,
                    now_epoch(),
                    token_count,
                    finish_reason,
                ],
            )?;
            let row_id = tx.last_insert_rowid();
            tx.execute(
                "UPDATE sessions SET message_count = message_count + 1 WHERE id = ?1",
                params![session_id.as_str()],
            )?;
            Ok(row_id)
        })
    }

    /// Accumulates provider-reported token usage and bumps the API-call
    /// counter. `reported=false` records the call separately so zero never
    /// implies the provider positively reported zero tokens.
    pub fn record_usage(
        &self,
        session_id: &SessionId,
        input_tokens: i64,
        output_tokens: i64,
        reported: bool,
    ) -> Result<(), StoreError> {
        self.with_write(|tx| {
            tx.execute(
                "UPDATE sessions SET input_tokens = input_tokens + ?1,
                        output_tokens = output_tokens + ?2,
                        api_call_count = api_call_count + 1,
                        unreported_usage_calls = unreported_usage_calls + ?3
                 WHERE id = ?4",
                params![
                    input_tokens,
                    output_tokens,
                    i64::from(!reported),
                    session_id.as_str()
                ],
            )?;
            Ok(())
        })
    }

    /// Tool invocations in the last `days` days, counted by tool name —
    /// derived from the messages ledger (tool-result rows carry `tool_name`),
    /// so usage-earned tool tiering (SPL §3.5) needs no separate counter table
    /// or write path.
    pub fn tool_use_counts(
        &self,
        days: f64,
    ) -> Result<std::collections::HashMap<String, u32>, StoreError> {
        self.with_read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT tool_name, COUNT(*) FROM messages
                 WHERE role = 'tool' AND tool_name IS NOT NULL AND timestamp > ?1
                 GROUP BY tool_name",
            )?;
            let rows = stmt.query_map(params![now_epoch() - days * 86_400.0], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?))
            })?;
            rows.collect()
        })
    }

    /// Assistant turns in the last `days` days — the denominator tool residency
    /// is priced against.
    ///
    /// A raw use count cannot say whether a tool is worth its schema: 21 uses is
    /// a lot in a hundred turns and nothing in four thousand. Measured here, one
    /// month of real use ran 4,152 turns, which made `create_document` cost
    /// ~6.2M tokens of residency to serve those 21 calls.
    pub fn assistant_turns(&self, days: f64) -> Result<u32, StoreError> {
        self.with_read(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM messages WHERE role = 'assistant' AND timestamp > ?1",
                params![now_epoch() - days * 86_400.0],
                |r| r.get(0),
            )
        })
    }

    /// Reconstructs the conversation in append order (for transcript replay).
    pub fn get_conversation(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<StoredMessage>, StoreError> {
        let exists: Option<String> = self.with_read(|conn| {
            conn.query_row(
                "SELECT id FROM sessions WHERE id = ?1",
                params![session_id.as_str()],
                |r| r.get(0),
            )
            .optional()
        })?;
        if exists.is_none() {
            return Err(StoreError::UnknownSession(session_id.to_string()));
        }
        self.with_read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, role, content, tool_call_id, tool_calls, tool_name, reasoning,
                        timestamp, finish_reason
                 FROM messages WHERE session_id = ?1 ORDER BY id",
            )?;
            let rows = stmt.query_map(params![session_id.as_str()], row_to_stored)?;
            rows.collect()
        })
    }

    /// Record a successful provider call that is not owned by an agent
    /// session (for example Distiller or Mixture-of-Models work).
    pub fn record_unscoped_usage(
        &self,
        input_tokens: i64,
        output_tokens: i64,
        reported: bool,
    ) -> Result<(), StoreError> {
        self.with_write(|tx| {
            tx.execute(
                "UPDATE usage_totals SET input_tokens = input_tokens + ?1,
                        output_tokens = output_tokens + ?2,
                        api_call_count = api_call_count + 1,
                        unreported_usage_calls = unreported_usage_calls + ?3
                 WHERE id = 1",
                params![input_tokens, output_tokens, i64::from(!reported)],
            )?;
            Ok(())
        })
    }
}

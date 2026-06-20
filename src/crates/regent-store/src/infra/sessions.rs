use crate::domain::entities::{InsightsRollup, StoredMessage};
use crate::domain::errors::StoreError;
use crate::infra::db::{Store, now_epoch};
use regent_kernel::{ChatMessage, Role, SessionId, ToolCall};
use rusqlite::{OptionalExtension, params};

impl Store {
    pub fn create_session(
        &self,
        id: &SessionId,
        source: &str,
        model: Option<&str>,
        system_prompt: Option<&str>,
        parent: Option<&SessionId>,
    ) -> Result<(), StoreError> {
        self.with_write(|tx| {
            tx.execute(
                "INSERT INTO sessions (id, source, model, system_prompt, parent_session_id, started_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    id.as_str(),
                    source,
                    model,
                    system_prompt,
                    parent.map(SessionId::as_str),
                    now_epoch()
                ],
            )?;
            Ok(())
        })
    }

    /// The frozen system prompt persisted at session creation (None for
    /// sessions created before schema v2).
    pub fn session_system_prompt(&self, id: &SessionId) -> Result<Option<String>, StoreError> {
        let row: Option<Option<String>> = self.with_read(|conn| {
            conn.query_row(
                "SELECT system_prompt FROM sessions WHERE id = ?1",
                params![id.as_str()],
                |r| r.get(0),
            )
            .optional()
        })?;
        row.ok_or_else(|| StoreError::UnknownSession(id.to_string()))
    }

    /// Records one completed turn (reproducibility: outcome + call count;
    /// the prompt and messages are already in `sessions`/`messages`).
    pub fn record_turn(
        &self,
        session_id: &SessionId,
        model: &str,
        api_calls: u32,
        outcome: &str,
        error: Option<&str>,
        started_at: f64,
    ) -> Result<(), StoreError> {
        self.with_write(|tx| {
            tx.execute(
                "INSERT INTO turns (session_id, model, api_calls, outcome, error, started_at, ended_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![session_id.as_str(), model, api_calls, outcome, error, started_at, now_epoch()],
            )?;
            Ok(())
        })
    }

    /// Aggregate usage across every session + the turns ledger — the data
    /// behind `regent insights`. Single read, two grouped aggregates.
    pub fn insights(&self) -> Result<InsightsRollup, StoreError> {
        self.with_read(|conn| {
            let (sessions, input_tokens, output_tokens, api_calls, messages) = conn.query_row(
                "SELECT COUNT(*), COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0),
                        COALESCE(SUM(api_call_count), 0), COALESCE(SUM(message_count), 0)
                 FROM sessions",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )?;
            let (turns, turns_ok) = conn.query_row(
                "SELECT COUNT(*), COALESCE(SUM(CASE WHEN outcome = 'ok' THEN 1 ELSE 0 END), 0)
                 FROM turns",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            Ok(InsightsRollup {
                sessions,
                turns,
                turns_ok,
                input_tokens,
                output_tokens,
                api_calls,
                messages,
            })
        })
    }

    pub fn end_session(&self, id: &SessionId, reason: &str) -> Result<(), StoreError> {
        self.with_write(|tx| {
            tx.execute(
                "UPDATE sessions SET ended_at = ?1, end_reason = ?2 WHERE id = ?3",
                params![now_epoch(), reason, id.as_str()],
            )?;
            Ok(())
        })
    }

    /// Appends one message and bumps the session counter. Returns the row id.
    pub fn append_message(
        &self,
        session_id: &SessionId,
        message: &ChatMessage,
        token_count: Option<i64>,
        finish_reason: Option<&str>,
    ) -> Result<i64, StoreError> {
        let tool_calls_json = if message.tool_calls.is_empty() {
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

    /// Accumulates token usage and bumps the API-call counter.
    pub fn record_usage(
        &self,
        session_id: &SessionId,
        input_tokens: i64,
        output_tokens: i64,
    ) -> Result<(), StoreError> {
        self.with_write(|tx| {
            tx.execute(
                "UPDATE sessions SET input_tokens = input_tokens + ?1,
                        output_tokens = output_tokens + ?2,
                        api_call_count = api_call_count + 1
                 WHERE id = ?3",
                params![input_tokens, output_tokens, session_id.as_str()],
            )?;
            Ok(())
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
}

fn row_to_stored(row: &rusqlite::Row<'_>) -> Result<StoredMessage, rusqlite::Error> {
    let role_text: String = row.get(1)?;
    let role = match role_text.as_str() {
        "user" => Role::User,
        "assistant" => Role::Assistant,
        "tool" => Role::Tool,
        other => {
            return Err(rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Text,
                format!("unknown role '{other}'").into(),
            ));
        }
    };
    let tool_calls: Vec<ToolCall> = match row.get::<_, Option<String>>(4)? {
        Some(json) => serde_json::from_str(&json).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                format!("tool_calls decode: {e}").into(),
            )
        })?,
        None => Vec::new(),
    };
    Ok(StoredMessage {
        id: row.get(0)?,
        message: ChatMessage {
            role,
            content: row.get(2)?,
            tool_calls,
            tool_call_id: row.get(3)?,
            tool_name: row.get(5)?,
            reasoning: row.get(6)?,
            // Not persisted: only the in-turn most-recent thinking block needs
            // replay (Anthropic doesn't validate older turns' thinking).
            thinking_signature: None,
        },
        timestamp: row.get(7)?,
        finish_reason: row.get(8)?,
    })
}

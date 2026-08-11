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

    /// Records the project folder this session opened, so a resume after an app
    /// restart points its tools at the same tree instead of falling back to the
    /// deacon's cwd. Overwrites: a session has one current workspace, no history.
    pub fn set_session_workspace(&self, id: &SessionId, workspace: &str) -> Result<(), StoreError> {
        self.with_write(|tx| {
            tx.execute(
                "UPDATE sessions SET workspace = ?1 WHERE id = ?2",
                params![workspace, id.as_str()],
            )?;
            Ok(())
        })
    }

    /// This session's opened workspace, or `None` when it has none — including
    /// for an unknown id, since `resume_session` reads this before it can know
    /// the row exists and a missing row must not abort the resume.
    pub fn session_workspace(&self, id: &SessionId) -> Result<Option<String>, StoreError> {
        let row = self.with_read(|conn| {
            conn.query_row(
                "SELECT workspace FROM sessions WHERE id = ?1",
                params![id.as_str()],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()
        })?;
        Ok(row.flatten())
    }

    /// ADR-038: stamps the session's birth prompt profile ("light"/"full").
    pub fn set_session_profile(&self, id: &SessionId, profile: &str) -> Result<(), StoreError> {
        self.with_write(|tx| {
            tx.execute(
                "UPDATE sessions SET profile = ?1 WHERE id = ?2",
                params![profile, id.as_str()],
            )?;
            Ok(())
        })
    }

    /// ADR-038 P2: records the moment a light session escalated to full. The
    /// birth `profile` stays "light" — escalation-rate = escalated/light.
    pub fn mark_session_escalated(&self, id: &SessionId) -> Result<(), StoreError> {
        self.with_write(|tx| {
            tx.execute(
                "UPDATE sessions SET escalated_at = ?1 WHERE id = ?2",
                params![now_epoch(), id.as_str()],
            )?;
            Ok(())
        })
    }

    /// ADR-038 P2: persists the escalated (full) prompt so a later resume
    /// restores the bytes the session actually ends on, not its light birth.
    pub fn update_session_prompt(&self, id: &SessionId, prompt: &str) -> Result<(), StoreError> {
        self.with_write(|tx| {
            tx.execute(
                "UPDATE sessions SET system_prompt = ?1 WHERE id = ?2",
                params![prompt, id.as_str()],
            )?;
            Ok(())
        })
    }

    /// ADR-038: the session's birth profile and whether it escalated —
    /// `(profile, escalated)`. `(None, false)` for pre-P1 or unknown sessions,
    /// so resume treats them as full.
    pub fn session_profile(&self, id: &SessionId) -> Result<(Option<String>, bool), StoreError> {
        let row = self.with_read(|conn| {
            conn.query_row(
                "SELECT profile, escalated_at IS NOT NULL FROM sessions WHERE id = ?1",
                params![id.as_str()],
                |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, bool>(1)?)),
            )
            .optional()
        })?;
        Ok(row.unwrap_or((None, false)))
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

    /// Number of persisted conversation messages successfully covered by the
    /// background learning reviewer. Zero for old/new sessions.
    pub fn session_reviewed_message_count(&self, id: &SessionId) -> Result<usize, StoreError> {
        let row: Option<i64> = self.with_read(|conn| {
            conn.query_row(
                "SELECT reviewed_message_count FROM sessions WHERE id = ?1",
                params![id.as_str()],
                |r| r.get(0),
            )
            .optional()
        })?;
        row.map(|value| usize::try_from(value.max(0)).unwrap_or(usize::MAX))
            .ok_or_else(|| StoreError::UnknownSession(id.to_string()))
    }

    /// Advances the durable learning cursor monotonically. A stale reviewer
    /// finishing after a newer one can never move the watermark backwards.
    pub fn advance_session_reviewed_message_count(
        &self,
        id: &SessionId,
        count: usize,
    ) -> Result<(), StoreError> {
        let count = i64::try_from(count).unwrap_or(i64::MAX);
        self.with_write(|tx| {
            tx.execute(
                "UPDATE sessions SET reviewed_message_count = MAX(reviewed_message_count, ?1) \
                 WHERE id = ?2",
                params![count, id.as_str()],
            )?;
            Ok(())
        })
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
            let (
                sessions,
                session_input_tokens,
                session_output_tokens,
                session_api_calls,
                session_unreported_usage_calls,
                messages,
            ): (i64, i64, i64, i64, i64, i64) = conn.query_row(
                "SELECT COUNT(*), COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0),
                        COALESCE(SUM(api_call_count), 0),
                        COALESCE(SUM(unreported_usage_calls), 0),
                        COALESCE(SUM(message_count), 0)
                 FROM sessions",
                [],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )?;
            let (
                global_input_tokens,
                global_output_tokens,
                global_api_calls,
                global_unreported_usage_calls,
                legacy_usage_unverified,
            ): (i64, i64, i64, i64, bool) = conn.query_row(
                "SELECT input_tokens, output_tokens, api_call_count,
                        unreported_usage_calls, legacy_unverified
                 FROM usage_totals WHERE id = 1",
                [],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get::<_, i64>(4)? != 0,
                    ))
                },
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
                input_tokens: session_input_tokens + global_input_tokens,
                output_tokens: session_output_tokens + global_output_tokens,
                api_calls: session_api_calls + global_api_calls,
                unreported_usage_calls: session_unreported_usage_calls
                    + global_unreported_usage_calls,
                legacy_usage_unverified,
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

    /// Deletes sessions that never produced anything: no messages, no turns,
    /// no children (a delegation parent must survive for its child's FK), and
    /// older than `min_age_secs` — the grace period protects a session another
    /// live process created moments ago and is about to use. Returns how many
    /// were removed. Run at deacon boot: abandoned "New Conversation" rows and
    /// gateway placeholder sessions otherwise accumulate forever in the rail.
    pub fn delete_empty_sessions(&self, min_age_secs: f64) -> Result<usize, StoreError> {
        self.with_write(|tx| {
            let n = tx.execute(
                "DELETE FROM sessions WHERE started_at < ?1
                   AND NOT EXISTS (SELECT 1 FROM messages WHERE session_id = sessions.id)
                   AND NOT EXISTS (SELECT 1 FROM turns WHERE session_id = sessions.id)
                   AND NOT EXISTS (SELECT 1 FROM sessions AS child
                                   WHERE child.parent_session_id = sessions.id)",
                params![now_epoch() - min_age_secs],
            )?;
            Ok(n)
        })
    }
}

pub(super) fn row_to_stored(row: &rusqlite::Row<'_>) -> Result<StoredMessage, rusqlite::Error> {
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

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;

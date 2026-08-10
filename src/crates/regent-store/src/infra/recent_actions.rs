//! Recency-ordered read path over the action log.
//!
//! `messages` has always been a complete record of what Regent did — every
//! tool call and every tool result is persisted verbatim. What was missing was
//! a way to read it by RECENCY: `search_messages` orders by relevance and
//! `list_sessions` returns titles only, so "pull up the last website we pulled
//! up" had no query behind it and the model answered "I have no record".

use crate::domain::entities::RecentAction;
use crate::domain::errors::StoreError;
use crate::infra::db::Store;
use rusqlite::types::Value as SqlValue;
use rusqlite::{OptionalExtension, params_from_iter};

/// A fat tool result (a whole file read, a terminal dump) must not blow the
/// context when ten of them arrive at once. `session_search` learned the same
/// lesson at 500.
const RESULT_MAX_CHARS: usize = 300;

/// The originating `tool_calls` payload is a JSON array, and a turn with several
/// parallel calls serialises well past the result cap — truncating it there
/// handed the model a blob cut mid-structure, with the URL it was looking for
/// often in the severed tail. Bounded far more generously, and only as a
/// backstop against a pathological payload.
/// ponytail: a plain char bound, not a parser. If truncation here ever matters,
/// select the ONE call whose name matches `tool_name` instead of widening again.
const ARGS_MAX_CHARS: usize = 2_000;

impl Store {
    /// Completed tool calls, newest first, across sessions.
    ///
    /// Cross-session by design: the Butler/voice surface runs its own session
    /// rows, so a site opened by voice is not in the chat transcript at all.
    /// Scoping this per-session would reproduce the exact bug it fixes.
    pub fn recent_actions(
        &self,
        tools: Option<&[String]>,
        limit: u32,
        since_epoch: Option<f64>,
    ) -> Result<Vec<RecentAction>, StoreError> {
        let mut sql = String::from(
            "SELECT m.id, m.timestamp, m.session_id, s.source, m.tool_name, m.content
             FROM messages m JOIN sessions s ON s.id = m.session_id
             WHERE m.tool_name IS NOT NULL",
        );
        let mut args: Vec<SqlValue> = Vec::new();
        if let Some(names) = tools.filter(|names| !names.is_empty()) {
            let holes = vec!["?"; names.len()].join(",");
            sql.push_str(&format!(" AND m.tool_name IN ({holes})"));
            args.extend(names.iter().map(|n| SqlValue::Text(n.clone())));
        }
        if let Some(since) = since_epoch {
            sql.push_str(" AND m.timestamp >= ?");
            args.push(SqlValue::Real(since));
        }
        sql.push_str(" ORDER BY m.timestamp DESC, m.id DESC LIMIT ?");
        args.push(SqlValue::Integer(i64::from(limit)));

        self.with_read(|conn| {
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params_from_iter(args.iter()), |row| {
                let id: i64 = row.get(0)?;
                let result: Option<String> = row.get(5)?;
                Ok((
                    id,
                    RecentAction {
                        timestamp: row.get(1)?,
                        session_id: row.get(2)?,
                        source: row.get(3)?,
                        tool_name: row.get(4)?,
                        result: cap(result.unwrap_or_default(), RESULT_MAX_CHARS),
                        args: None,
                    },
                ))
            })?;
            let found: Vec<(i64, RecentAction)> = rows.collect::<Result<_, _>>()?;

            // Not every tool echoes its input into its result. The call that
            // produced this row is the nearest preceding assistant message
            // carrying `tool_calls` in the same session.
            let mut out = Vec::with_capacity(found.len());
            for (id, mut action) in found {
                action.args = conn
                    .query_row(
                        "SELECT tool_calls FROM messages
                         WHERE session_id = ?1 AND id < ?2 AND tool_calls IS NOT NULL
                         ORDER BY id DESC LIMIT 1",
                        rusqlite::params![action.session_id, id],
                        |row| row.get::<_, Option<String>>(0),
                    )
                    .optional()?
                    .flatten()
                    .map(|args| cap(args, ARGS_MAX_CHARS));
                out.push(action);
            }
            Ok(out)
        })
    }
}

fn cap(text: String, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text,
    }
}

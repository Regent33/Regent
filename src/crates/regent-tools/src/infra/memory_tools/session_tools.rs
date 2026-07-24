//! session_search / session_list — episodic recall over the session store.
//! Split from `memory_tools.rs` (file-size rule).

use super::*;

/// Upper bound on `session_search` hits: recall is a starting point, not a
/// full-history dump, and it keeps the tool's output from crowding context.
const SESSION_SEARCH_MAX: u32 = 20;
/// Per-hit snippet char budget (UTF-8-safe); FTS gives ~16 tokens, but a
/// pathological message can still be long. Matches web_search's bound.
const SNIPPET_MAX_CHARS: usize = 500;

/// Default 10 hits; clamp any request into `1..=SESSION_SEARCH_MAX` so an
/// oversized `limit` can't turn recall into a full-history dump.
fn clamp_session_limit(requested: Option<u64>) -> u32 {
    requested.unwrap_or(10).clamp(1, SESSION_SEARCH_MAX as u64) as u32
}

/// Cap a snippet to a fixed CHAR budget (UTF-8-safe), appending `…` when cut.
fn cap_snippet(snippet: &str) -> String {
    if snippet.chars().count() <= SNIPPET_MAX_CHARS {
        return snippet.to_owned();
    }
    let head: String = snippet.chars().take(SNIPPET_MAX_CHARS).collect();
    format!("{head}…")
}

pub(super) fn session_search_definition() -> ToolDefinition {
    ToolDefinition {
        name: "session_search".into(),
        description: "Full-text search across all past conversations. Start with ONE broad \
                      query (keywords only), then refine only if the first pass misses. Use \
                      when the user references something from an earlier session."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Keywords; supports AND/OR/NOT and \"phrases\"."},
                "limit": {"type": "integer", "description": "Max hits (default 10, max 20)."}
            },
            "required": ["query"]
        }),
        toolset: "memory".into(),
    }
}

pub(super) fn session_list_definition() -> ToolDefinition {
    ToolDefinition {
        name: "session_list".into(),
        description: "Past sessions newest-first (title, surface, start time, messages). For \
                      time-based recall ('what did we do today?'); drill in with session_search."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "limit": {"type": "integer", "description": "Max sessions (default 20)."},
                "day": {"type": "string", "description": "YYYY-MM-DD (local): only that day."}
            }
        }),
        toolset: "memory".into(),
    }
}

pub(super) struct SessionListTool {
    pub(super) store: Arc<Store>,
}

/// Recent sessions as JSON rows — shared by `session_list` and
/// `session_search`'s browse fallback.
fn sessions_json(store: &Store, limit: usize, day: Option<&str>) -> String {
    // Over-fetch when day-filtering so a busy history still fills the day.
    let fetch = if day.is_some() { limit.max(200) } else { limit };
    match store.list_sessions(fetch) {
        Ok(sessions) => {
            let rows: Vec<Value> = sessions
                .iter()
                .filter(|s| match day {
                    Some(d) => local_day(s.started_at) == *d,
                    None => true,
                })
                .take(limit)
                .map(|s| {
                    json!({
                        "session_id": s.id,
                        "title": s.title,
                        "surface": s.source,
                        "started_local": local_stamp(s.started_at),
                        "messages": s.message_count,
                    })
                })
                .collect();
            json!({"sessions": rows, "count": rows.len()}).to_string()
        }
        Err(error) => tool_error_json(error.to_string()),
    }
}

#[async_trait]
impl ToolExecutor for SessionListTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(20) as usize;
        let day = args
            .get("day")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let store = Arc::clone(&self.store);
        tokio::task::spawn_blocking(move || Ok(sessions_json(&store, limit, day.as_deref())))
            .await
            .map_err(|e| RegentError::Tool {
                tool: "session_list".into(),
                message: e.to_string(),
            })?
    }
}

/// Epoch seconds → the user's local "YYYY-MM-DD" (matching the `day` filter).
fn local_day(epoch: f64) -> String {
    stamp(epoch, "%Y-%m-%d")
}

/// Epoch seconds → a readable local timestamp for the listing.
fn local_stamp(epoch: f64) -> String {
    stamp(epoch, "%Y-%m-%d %H:%M")
}

fn stamp(epoch: f64, fmt: &str) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(epoch as i64, 0)
        .single()
        .map(|t| t.format(fmt).to_string())
        .unwrap_or_default()
}

pub(super) struct SessionSearchTool {
    pub(super) store: Arc<Store>,
}

#[async_trait]
impl ToolExecutor for SessionSearchTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(query) = args
            .get("query")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
        else {
            return Ok(tool_error_json("missing required parameter: query"));
        };
        let limit = clamp_session_limit(args.get("limit").and_then(Value::as_u64));
        let store = Arc::clone(&self.store);
        // A wildcard/garbage query ("*") has nothing to search — models send
        // it meaning "show me everything", and a literal zero-hit answer reads
        // as "no past sessions exist" (owner repro 2026-07-17). Browse instead.
        if regent_store::natural_fts_query(&query).is_empty() {
            return tokio::task::spawn_blocking(move || {
                let listing = sessions_json(&store, limit as usize, None);
                Ok(json!({
                    "note": "query had no searchable keywords — listing recent sessions instead \
                             (use session_list for browsing, session_search for keywords)",
                    "recent": serde_json::from_str::<Value>(&listing).unwrap_or(Value::Null),
                })
                .to_string())
            })
            .await
            .map_err(|e| RegentError::Tool {
                tool: "session_search".into(),
                message: e.to_string(),
            })?;
        }
        tokio::task::spawn_blocking(move || match store.search_messages(&query, limit) {
            Ok(hits) => {
                let results: Vec<Value> = hits
                    .iter()
                    .map(|hit| {
                        json!({
                            "session_id": hit.session_id,
                            "role": hit.role,
                            "snippet": cap_snippet(&hit.snippet),
                            "timestamp": hit.timestamp,
                        })
                    })
                    .collect();
                Ok(json!({"results": results, "count": results.len()}).to_string())
            }
            Err(error) => Ok(tool_error_json(error.to_string())),
        })
        .await
        .map_err(|e| RegentError::Tool {
            tool: "session_search".into(),
            message: e.to_string(),
        })?
    }
}

#[cfg(test)]
#[path = "../tests/session_tools.rs"]
mod tests;

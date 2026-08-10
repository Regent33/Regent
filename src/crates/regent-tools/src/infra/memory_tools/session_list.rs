//! `session_list` — browsing past sessions, plus the JSON rows
//! `session_search` falls back to when a query has no searchable keywords.
//! Split from `session_tools.rs` (file-size rule).

use super::*;

pub(super) fn session_list_definition() -> ToolDefinition {
    ToolDefinition {
        name: "session_list".into(),
        description: "Past sessions newest-first. Time-based recall; drill in with \
                      session_search. `actions` returns what you recently DID instead — check \
                      it before claiming no record of something recent."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "limit": {"type": "integer", "description": "Max rows (default 20)."},
                "day": {"type": "string", "description": "YYYY-MM-DD (local)."},
                "actions": {"type": "string", "description": "'all' or a tool, e.g. 'open_url'."}
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
pub(super) fn sessions_json(store: &Store, limit: usize, day: Option<&str>) -> String {
    // Always over-fetch, not just when day-filtering: internal sessions
    // outnumber real ones, so fetching exactly `limit` and then filtering
    // returned a handful of rows and looked like an empty history.
    let fetch = limit.max(200);
    match store.list_sessions(fetch) {
        Ok(sessions) => {
            let rows: Vec<Value> = sessions
                .iter()
                .filter(|s| s.is_user_facing())
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
        // Recall's recency lane rides this tool rather than a new one: a new
        // resident schema does not fit the model-facing catalog ceiling
        // (deacon_basics::tiering, 3.15k), and this is already THE recall-browse
        // tool a light session reaches for.
        let actions = args
            .get("actions")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let store = Arc::clone(&self.store);
        tokio::task::spawn_blocking(move || {
            Ok(match actions {
                Some(filter) => actions_json(&store, limit.clamp(1, 25), &filter),
                None => sessions_json(&store, limit, day.as_deref()),
            })
        })
        .await
        .map_err(|e| RegentError::Tool {
            tool: "session_list".into(),
            message: e.to_string(),
        })?
    }
}

/// Recent tool calls + results, newest first, ACROSS sessions.
///
/// Cross-session is the point: the Butler voice surface owns its own session
/// rows, so a site it opened is not in the chat transcript at all. Scoping this
/// to the asking session would reproduce the bug it exists to fix.
pub(super) fn actions_json(store: &Store, limit: usize, filter: &str) -> String {
    let tools: Vec<String> = filter
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case("all"))
        .map(ToOwned::to_owned)
        .collect();
    let names = (!tools.is_empty()).then_some(tools.as_slice());
    match store.recent_actions(names, limit as u32, None) {
        Ok(actions) => {
            let rows: Vec<Value> = actions
                .iter()
                .map(|action| {
                    json!({
                        "when_local": local_stamp(action.timestamp),
                        "surface": action.source,
                        "session_id": action.session_id,
                        "tool": action.tool_name,
                        "args": action.args,
                        "result": action.result,
                    })
                })
                .collect();
            json!({"actions": rows, "count": rows.len()}).to_string()
        }
        Err(error) => tool_error_json(error.to_string()),
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

//! `session_list` — browsing past sessions, plus the JSON rows
//! `session_search` falls back to when a query has no searchable keywords.
//! Split from `session_tools.rs` (file-size rule).

use super::*;

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

/// Surfaces that are Regent talking to itself, not a conversation the user had.
/// A denylist rather than an allowlist on purpose: a new user-facing surface
/// should show up by default, not vanish until someone remembers to list it.
///
/// This is not a tidiness rule. On the owner's live store these outnumber real
/// conversations 4:1 — 669 `review` + 183 `background` against 184 `deacon` —
/// so 15 of the 20 newest rows were internal noise or empty, and "what did we
/// do this week?" was answered off a list three-quarters full of the learning
/// loop's own sessions, most with blank titles.
const INTERNAL_SURFACES: &[&str] = &["review", "background", "delegate"];

fn user_facing(source: &str, message_count: i64) -> bool {
    // A zero-message session is a session that never happened.
    message_count > 0 && !INTERNAL_SURFACES.contains(&source)
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
                .filter(|s| user_facing(&s.source, s.message_count))
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

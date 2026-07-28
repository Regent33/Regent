//! session_search / session_list — episodic recall over the session store.
//! Split from `memory_tools.rs` (file-size rule).

use super::*;

/// Upper bound on `session_search` hits: recall is a starting point, not a
/// full-history dump, and it keeps the tool's output from crowding context.
const SESSION_SEARCH_MAX: u32 = 20;
/// Per-hit snippet char budget (UTF-8-safe); FTS gives ~16 tokens, but a
/// pathological message can still be long. Matches web_search's bound.
const SNIPPET_MAX_CHARS: usize = 500;

/// W11: how many turns either side of a hit come back with it.
///
/// An FTS snippet is ~16 tokens, which for a short turn is the whole message
/// and still unreadable — *"yes, do that"* answers a question the hit does not
/// contain, and the model's only recourse was another search that could not
/// find it either.
///
/// One, not five. Context is charged per hit against a result that already
/// allows 20 of them, so a radius of 5 would multiply this tool's output by an
/// order of magnitude to answer a question usually settled by the turn
/// immediately before.
const CONTEXT_RADIUS_DEFAULT: u32 = 1;
const CONTEXT_RADIUS_MAX: u32 = 3;

/// Char budget per context turn. Deliberately tighter than the snippet: these
/// are there to make the hit *legible*, not to be read in full.
const CONTEXT_MAX_CHARS: usize = 220;

/// Clamp the requested radius. `0` is honoured — it restores the exact
/// pre-W11 payload for a caller that only wants hits.
fn clamp_context_radius(requested: Option<u64>) -> u32 {
    requested
        .unwrap_or(CONTEXT_RADIUS_DEFAULT as u64)
        .min(CONTEXT_RADIUS_MAX as u64) as u32
}

/// Cap any text to a char budget (UTF-8-safe), appending `…` when cut.
fn cap_chars(text: &str, budget: usize) -> String {
    if text.chars().count() <= budget {
        return text.to_owned();
    }
    let head: String = text.chars().take(budget).collect();
    format!("{head}…")
}

/// Default 10 hits; clamp any request into `1..=SESSION_SEARCH_MAX` so an
/// oversized `limit` can't turn recall into a full-history dump.
fn clamp_session_limit(requested: Option<u64>) -> u32 {
    requested.unwrap_or(10).clamp(1, SESSION_SEARCH_MAX as u64) as u32
}

/// Cap a snippet to a fixed CHAR budget (UTF-8-safe), appending `…` when cut.
fn cap_snippet(snippet: &str) -> String {
    cap_chars(snippet, SNIPPET_MAX_CHARS)
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
                "limit": {"type": "integer", "description": "Max hits (default 10, max 20)."},
                "context": {"type": "integer", "description": "Turns of surrounding conversation per hit (default 1, max 3). 0 for snippets only."}
            },
            "required": ["query"]
        }),
        toolset: "memory".into(),
    }
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
                let listing = super::session_list::sessions_json(&store, limit as usize, None);
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
        let radius = clamp_context_radius(args.get("context").and_then(Value::as_u64));
        tokio::task::spawn_blocking(move || match store.search_messages(&query, limit) {
            Ok(hits) => {
                let results: Vec<Value> = hits
                    .iter()
                    .map(|hit| {
                        let mut row = json!({
                            "session_id": hit.session_id,
                            "role": hit.role,
                            "snippet": cap_snippet(&hit.snippet),
                            "timestamp": hit.timestamp,
                        });
                        // W11: the turns either side, so a hit like "yes, do
                        // that" is legible without a second search. Omitted
                        // entirely at radius 0 or when the hit stands alone,
                        // so the payload never carries an empty key.
                        if let Some(obj) = row.as_object_mut()
                            && radius > 0
                        {
                            // Best-effort: a window read that fails leaves the
                            // hit exactly as it was before W11. Recall must not
                            // break because context could not be fetched.
                            let window = store
                                .message_window(&hit.session_id, hit.message_id, radius)
                                .unwrap_or_default();
                            if !window.is_empty() {
                                let turns: Vec<Value> = window
                                    .iter()
                                    .map(|m| {
                                        json!({
                                            "offset": m.offset,
                                            "role": m.role,
                                            "text": cap_chars(&m.content, CONTEXT_MAX_CHARS),
                                        })
                                    })
                                    .collect();
                                obj.insert("context".into(), Value::Array(turns));
                            }
                        }
                        row
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

#[cfg(test)]
#[path = "../tests/session_context.rs"]
mod context_tests;

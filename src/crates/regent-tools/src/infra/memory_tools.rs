//! Memory-facing core tools: `memory` (bounded add/replace/remove with
//! budget semantics), `memory_search` (hybrid graph recall), and
//! `session_search` (FTS over all past conversations). Registered via
//! [`register_memory_tools`] by whoever owns the store handles — the model
//! never sees the difference from any other tool.

use crate::application::catalog::ToolCatalog;
use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_graph::{AddOutcome, GraphError, GraphMemory, MemoryTarget, Provenance};
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use regent_store::Store;
use serde_json::{Value, json};
use std::sync::Arc;

pub fn register_memory_tools(
    catalog: &mut ToolCatalog,
    graph: Arc<GraphMemory>,
    store: Arc<Store>,
) -> Result<(), RegentError> {
    catalog.register(
        memory_definition(),
        Arc::new(MemoryTool {
            graph: Arc::clone(&graph),
        }),
    )?;
    catalog.register(
        memory_search_definition(),
        Arc::new(MemorySearchTool { graph }),
    )?;
    catalog.register(
        session_search_definition(),
        Arc::new(SessionSearchTool { store }),
    )?;
    Ok(())
}

fn memory_definition() -> ToolDefinition {
    ToolDefinition {
        name: "memory".into(),
        description: "Save durable facts across sessions. Targets: 'memory' (environment, \
                      conventions, lessons) and 'user' (identity, preferences). Actions: add, \
                      replace, remove — replace/remove match one entry by a unique substring \
                      (old_text). Stores have hard char limits; on overflow, consolidate \
                      existing entries in this same turn and retry."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "action": {"type": "string", "enum": ["add", "replace", "remove"]},
                "target": {"type": "string", "enum": ["memory", "user"]},
                "content": {"type": "string", "description": "Entry text (add/replace)."},
                "old_text": {"type": "string", "description": "Unique substring of the entry to replace/remove."}
            },
            "required": ["action", "target"]
        }),
        toolset: "memory".into(),
    }
}

struct MemoryTool {
    graph: Arc<GraphMemory>,
}

#[async_trait]
impl ToolExecutor for MemoryTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let graph = Arc::clone(&self.graph);
        // A sandboxed context marks an externally-triggered session (platform
        // webhooks / gateway) — its memory writes go through the §10.2
        // approval gate instead of committing directly.
        let external = ctx.is_sandboxed();
        // Graph calls are blocking SQLite underneath.
        tokio::task::spawn_blocking(move || Ok(run_memory_action(&graph, &args, external)))
            .await
            .map_err(|e| RegentError::Tool {
                tool: "memory".into(),
                message: e.to_string(),
            })?
    }
}

/// Seven days for an external write proposal to be approved before it expires.
const PENDING_WRITE_TTL_SECS: f64 = 7.0 * 86_400.0;

fn run_memory_action(graph: &GraphMemory, args: &Value, external: bool) -> String {
    let action = args
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let target = match MemoryTarget::parse(
        args.get("target")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    ) {
        Ok(target) => target,
        Err(error) => return tool_error_json(error.to_string()),
    };
    let content = args.get("content").and_then(Value::as_str);
    let old_text = args.get("old_text").and_then(Value::as_str);

    if external {
        // External sessions may only PROPOSE additions (staged for the owner
        // to approve via `memory.pending`/`memory.approve`); edits to what is
        // already trusted memory are refused outright.
        return match (action, content) {
            ("add", Some(content)) => match graph.stage_write(
                regent_graph::ENTRY_KIND,
                target.kind(),
                content,
                Provenance::AgentInferred,
                None,
                Some(PENDING_WRITE_TTL_SECS),
            ) {
                Ok(id) => json!({
                    "success": true,
                    "result": format!(
                        "queued for the owner's approval (id {id}); it is NOT saved yet"),
                })
                .to_string(),
                Err(error) => tool_error_json(error.to_string()),
            },
            _ => tool_error_json(
                "memory edits from an externally-triggered session require the owner: \
                 only 'add' is accepted here, and it is queued for approval",
            ),
        };
    }

    let outcome = match (action, content, old_text) {
        ("add", Some(content), _) => graph.add_entry(target, content).map(|added| match added {
            AddOutcome::Added => "saved".to_owned(),
            AddOutcome::Duplicate => "already stored — no duplicate added".to_owned(),
        }),
        ("replace", Some(content), Some(old_text)) => graph
            .replace_entry(target, old_text, content)
            .map(|()| "replaced".to_owned()),
        ("remove", _, Some(old_text)) => graph
            .remove_entry(target, old_text)
            .map(|()| "removed".to_owned()),
        _ => {
            return tool_error_json(
                "invalid arguments: add needs content; replace needs old_text + content; \
             remove needs old_text",
            );
        }
    };

    match outcome {
        Ok(message) => {
            let (used, limit) = graph.usage(target).unwrap_or((0, 0));
            json!({"success": true, "result": message, "usage": format!("{used}/{limit}")})
                .to_string()
        }
        // The budget error carries current entries so the agent can
        // consolidate in the same turn (never auto-compacted).
        Err(GraphError::BudgetExceeded {
            used,
            limit,
            attempted,
            entries,
        }) => json!({
            "success": false,
            "error": format!(
                "Memory at {used}/{limit} chars. This write ({attempted} chars) would exceed \
                 the limit. Consolidate now: 'replace' overlapping entries with shorter ones or \
                 'remove' stale ones (see current_entries), then retry — all in this turn."),
            "current_entries": entries,
            "usage": format!("{used}/{limit}"),
        })
        .to_string(),
        Err(error) => tool_error_json(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph() -> GraphMemory {
        GraphMemory::new(Arc::new(Store::open_in_memory().unwrap()))
    }

    /// P1-003: an external session's `memory add` must stage, not commit;
    /// approval commits it through the normal entry path.
    #[test]
    fn external_add_is_staged_until_approved() {
        let graph = graph();
        let args = json!({"action": "add", "target": "memory", "content": "likes tabs"});

        let reply = run_memory_action(&graph, &args, true);
        assert!(reply.contains("queued"), "got: {reply}");
        let (used, _) = graph.usage(MemoryTarget::Memory).unwrap();
        assert_eq!(used, 0, "nothing committed yet");

        let pending = graph.pending_writes(10).unwrap();
        assert_eq!(pending.len(), 1);
        graph
            .approve_write(&pending[0].id)
            .unwrap()
            .expect("committed");
        let (used, _) = graph.usage(MemoryTarget::Memory).unwrap();
        assert!(used > 0, "approved entry landed");
    }

    #[test]
    fn external_replace_and_remove_are_refused_but_local_add_commits() {
        let graph = graph();
        let replace = json!({"action": "replace", "target": "memory",
                             "content": "x", "old_text": "y"});
        assert!(run_memory_action(&graph, &replace, true).contains("error"));

        let add = json!({"action": "add", "target": "memory", "content": "local fact"});
        assert!(run_memory_action(&graph, &add, false).contains("saved"));
        assert!(
            graph.pending_writes(10).unwrap().is_empty(),
            "local writes don't stage"
        );
    }
}

fn memory_search_definition() -> ToolDefinition {
    ToolDefinition {
        name: "memory_search".into(),
        description: "Search long-term graph memory (facts, entities, episodes) by topic. \
                      Returns provenance-labeled matches as reference data."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "k": {"type": "integer", "description": "Max results (default 5)."}
            },
            "required": ["query"]
        }),
        toolset: "memory".into(),
    }
}

struct MemorySearchTool {
    graph: Arc<GraphMemory>,
}

#[async_trait]
impl ToolExecutor for MemorySearchTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(query) = args
            .get("query")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
        else {
            return Ok(tool_error_json("missing required parameter: query"));
        };
        let k = args.get("k").and_then(Value::as_u64).unwrap_or(5) as usize;
        let graph = Arc::clone(&self.graph);
        tokio::task::spawn_blocking(move || match graph.retrieve(&query, k) {
            Ok(results) => Ok(json!({
                "rendered": GraphMemory::render_recall(&results),
                "count": results.len(),
            })
            .to_string()),
            Err(error) => Ok(tool_error_json(error.to_string())),
        })
        .await
        .map_err(|e| RegentError::Tool {
            tool: "memory_search".into(),
            message: e.to_string(),
        })?
    }
}

fn session_search_definition() -> ToolDefinition {
    ToolDefinition {
        name: "session_search".into(),
        description: "Full-text search across all past conversations. Use when the user \
                      references something from an earlier session."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Keywords; supports AND/OR/NOT and \"phrases\"."},
                "limit": {"type": "integer", "description": "Max hits (default 10)."}
            },
            "required": ["query"]
        }),
        toolset: "memory".into(),
    }
}

struct SessionSearchTool {
    store: Arc<Store>,
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
        let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(10) as u32;
        let store = Arc::clone(&self.store);
        tokio::task::spawn_blocking(move || match store.search_messages(&query, limit) {
            Ok(hits) => {
                let results: Vec<Value> = hits
                    .iter()
                    .map(|hit| {
                        json!({
                            "session_id": hit.session_id,
                            "role": hit.role,
                            "snippet": hit.snippet,
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

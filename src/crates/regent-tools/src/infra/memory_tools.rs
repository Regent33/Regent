//! Memory-facing core tools: `memory` (bounded add/replace/remove with
//! Hermes budget semantics), `memory_search` (hybrid graph recall), and
//! `session_search` (FTS over all past conversations). Registered via
//! [`register_memory_tools`] by whoever owns the store handles — the model
//! never sees the difference from any other tool.

use crate::application::catalog::ToolCatalog;
use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_graph::{AddOutcome, GraphError, GraphMemory, MemoryTarget};
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use regent_store::Store;
use serde_json::{Value, json};
use std::sync::Arc;

pub fn register_memory_tools(
    catalog: &mut ToolCatalog,
    graph: Arc<GraphMemory>,
    store: Arc<Store>,
) -> Result<(), RegentError> {
    catalog.register(memory_definition(), Arc::new(MemoryTool { graph: Arc::clone(&graph) }))?;
    catalog.register(memory_search_definition(), Arc::new(MemorySearchTool { graph }))?;
    catalog.register(session_search_definition(), Arc::new(SessionSearchTool { store }))?;
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
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let graph = Arc::clone(&self.graph);
        // Graph calls are blocking SQLite underneath.
        tokio::task::spawn_blocking(move || Ok(run_memory_action(&graph, &args)))
            .await
            .map_err(|e| RegentError::Tool { tool: "memory".into(), message: e.to_string() })?
    }
}

fn run_memory_action(graph: &GraphMemory, args: &Value) -> String {
    let action = args.get("action").and_then(Value::as_str).unwrap_or_default();
    let target = match MemoryTarget::parse(args.get("target").and_then(Value::as_str).unwrap_or_default()) {
        Ok(target) => target,
        Err(error) => return tool_error_json(error.to_string()),
    };
    let content = args.get("content").and_then(Value::as_str);
    let old_text = args.get("old_text").and_then(Value::as_str);

    let outcome = match (action, content, old_text) {
        ("add", Some(content), _) => graph.add_entry(target, content).map(|added| match added {
            AddOutcome::Added => "saved".to_owned(),
            AddOutcome::Duplicate => "already stored — no duplicate added".to_owned(),
        }),
        ("replace", Some(content), Some(old_text)) => graph
            .replace_entry(target, old_text, content)
            .map(|()| "replaced".to_owned()),
        ("remove", _, Some(old_text)) => {
            graph.remove_entry(target, old_text).map(|()| "removed".to_owned())
        }
        _ => return tool_error_json(
            "invalid arguments: add needs content; replace needs old_text + content; \
             remove needs old_text",
        ),
    };

    match outcome {
        Ok(message) => {
            let (used, limit) = graph.usage(target).unwrap_or((0, 0));
            json!({"success": true, "result": message, "usage": format!("{used}/{limit}")})
                .to_string()
        }
        // The budget error carries current entries so the agent can
        // consolidate in the same turn (never auto-compacted).
        Err(GraphError::BudgetExceeded { used, limit, attempted, entries }) => json!({
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
        let Some(query) = args.get("query").and_then(Value::as_str).map(ToOwned::to_owned) else {
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
        .map_err(|e| RegentError::Tool { tool: "memory_search".into(), message: e.to_string() })?
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
        let Some(query) = args.get("query").and_then(Value::as_str).map(ToOwned::to_owned) else {
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
        .map_err(|e| RegentError::Tool { tool: "session_search".into(), message: e.to_string() })?
    }
}

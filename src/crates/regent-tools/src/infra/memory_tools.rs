//! Memory-facing core tools: `memory` (bounded add/replace/remove with
//! budget semantics), `memory_search` (hybrid graph recall),
//! `session_search` (FTS over all past conversations), and `session_list`
//! (time-ordered recall — "what did we do today"). Registered via
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
        Arc::new(SessionSearchTool {
            store: Arc::clone(&store),
        }),
    )?;
    catalog.register(
        session_list_definition(),
        Arc::new(SessionListTool { store }),
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

use session_tools::{
    SessionListTool, SessionSearchTool, session_list_definition, session_search_definition,
};

use actions::run_memory_action;

mod actions;
mod session_tools;

#[cfg(test)]
#[path = "memory_tools_tests.rs"]
mod tests;

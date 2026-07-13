//! `kanban` — the worker toolset over the shared board. The agent creates,
//! claims, and moves tasks; the store guarantees claims are atomic (one
//! winner). Board-scoped at registration so tenants/projects stay isolated.

use crate::application::catalog::ToolCatalog;
use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use regent_store::Store;
use serde_json::{Value, json};
use std::sync::Arc;

/// Registers `kanban`, scoped to `board`. `worker_id` is the default assignee
/// used when claiming (a worker profile identifies itself once, here).
pub fn register_kanban_tool(
    catalog: &mut ToolCatalog,
    store: Arc<Store>,
    board: String,
    worker_id: String,
) -> Result<(), RegentError> {
    catalog.register(
        kanban_definition(),
        Arc::new(KanbanTool {
            store,
            board,
            worker_id,
        }),
    )
}

fn kanban_definition() -> ToolDefinition {
    ToolDefinition {
        name: "kanban".into(),
        description: "Manage tasks on the shared work board. Actions: create (title, \
                      description), list (optional status filter), claim (id), submit (id — \
                      finished work, send to review), approve (id — review passed, mark done), \
                      reject (id — review failed, send back to in_progress), block (id). Tasks \
                      flow todo → in_progress → in_review → done; block is reachable from any \
                      column. Claiming is atomic, so only one worker can own a task. Work is \
                      reviewed before it's marked done — never approve your own task without a \
                      review step."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create", "list", "claim", "submit", "approve", "reject", "block"]
                },
                "title": {"type": "string", "description": "Task title (create)."},
                "description": {"type": "string", "description": "Task detail (create)."},
                "id": {"type": "string", "description": "Task id (claim/submit/approve/reject/block)."},
                "status": {"type": "string", "description": "Status filter for list (todo/in_progress/in_review/done/blocked)."}
            },
            "required": ["action"]
        }),
        toolset: "kanban".into(),
    }
}

struct KanbanTool {
    store: Arc<Store>,
    board: String,
    worker_id: String,
}

#[async_trait]
impl ToolExecutor for KanbanTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let store = Arc::clone(&self.store);
        let board = self.board.clone();
        let worker = self.worker_id.clone();
        // Store calls are blocking SQLite.
        tokio::task::spawn_blocking(move || Ok(run_kanban_action(&store, &board, &worker, &args)))
            .await
            .map_err(|e| RegentError::Tool {
                tool: "kanban".into(),
                message: e.to_string(),
            })?
    }
}

fn run_kanban_action(store: &Store, board: &str, worker: &str, args: &Value) -> String {
    let action = args
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let result = match action {
        "create" => create(store, board, args),
        "list" => list(store, board, args),
        "claim" => claim(store, worker, args),
        // Guarded review flow: each step only fires from the expected column.
        "submit" => transition(store, args, "in_progress", "in_review"),
        "approve" => transition(store, args, "in_review", "done"),
        "reject" => transition(store, args, "in_review", "in_progress"),
        // Block is valid from any column.
        "block" => move_to(store, args, "blocked"),
        other => return tool_error_json(format!("unknown kanban action '{other}'")),
    };
    match result {
        Ok(value) => value.to_string(),
        Err(message) => tool_error_json(message),
    }
}

fn create(store: &Store, board: &str, args: &Value) -> Result<Value, String> {
    let Some(title) = args.get("title").and_then(Value::as_str) else {
        return Err("create needs a title".into());
    };
    let description = args
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("");
    let id = format!("task_{}", uuid::Uuid::new_v4().simple());
    store
        .create_task(&id, board, title, description)
        .map_err(|e| e.to_string())?;
    Ok(json!({"success": true, "id": id}))
}

fn list(store: &Store, board: &str, args: &Value) -> Result<Value, String> {
    let status = args.get("status").and_then(Value::as_str);
    let tasks = store.list_tasks(board, status).map_err(|e| e.to_string())?;
    let items: Vec<Value> = tasks
        .iter()
        .map(|t| json!({"id": t.id, "title": t.title, "status": t.status, "assignee": t.assignee}))
        .collect();
    Ok(json!({"tasks": items, "count": items.len()}))
}

fn claim(store: &Store, worker: &str, args: &Value) -> Result<Value, String> {
    let Some(id) = args.get("id").and_then(Value::as_str) else {
        return Err("claim needs a task id".into());
    };
    let claimed = store.claim_task(id, worker).map_err(|e| e.to_string())?;
    Ok(json!({"success": claimed, "claimed": claimed, "assignee": worker}))
}

fn move_to(store: &Store, args: &Value, status: &str) -> Result<Value, String> {
    let Some(id) = args.get("id").and_then(Value::as_str) else {
        return Err("this action needs a task id".into());
    };
    let moved = store
        .set_task_status(id, status)
        .map_err(|e| e.to_string())?;
    Ok(json!({"success": moved, "status": status}))
}

/// A guarded column transition. `success` is false when the task isn't in the
/// expected `from` column (e.g. approving something that was never submitted).
fn transition(store: &Store, args: &Value, from: &str, to: &str) -> Result<Value, String> {
    let Some(id) = args.get("id").and_then(Value::as_str) else {
        return Err("this action needs a task id".into());
    };
    let moved = store
        .transition_task(id, from, to)
        .map_err(|e| e.to_string())?;
    Ok(json!({"success": moved, "status": if moved { to } else { from }}))
}

#[cfg(test)]
#[path = "kanban_tools_tests.rs"]
mod tests;

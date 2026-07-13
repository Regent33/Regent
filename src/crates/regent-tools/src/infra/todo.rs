//! `todo_write` (gap T2) — the model's working plan for a multi-step task.
//! Each call replaces the whole list and echoes it back rendered, so the plan
//! stays visible in recent context.
// ponytail: per-session in-memory state (one executor instance per session
// catalog), no todos.json — a shared file would cross-clobber between
// concurrent sessions, and a working list needn't survive a restart.

use crate::ToolCatalog;
use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "todo_write".into(),
        description: "Maintain your working plan for a multi-step task: send the FULL list \
             each call (it replaces the previous one), each item with a status. Keep exactly \
             one item in_progress; mark items completed as you finish them."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": {"type": "string"},
                            "status": {"type": "string", "enum": ["pending", "in_progress", "completed"]}
                        },
                        "required": ["content", "status"]
                    }
                }
            },
            "required": ["todos"]
        }),
        toolset: "core".into(),
    }
}

#[derive(Default)]
struct TodoTool {
    todos: Mutex<Vec<(String, String)>>,
}

#[async_trait]
impl ToolExecutor for TodoTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(items) = args.get("todos").and_then(Value::as_array) else {
            return Ok(tool_error_json("todo_write needs 'todos' (an array)"));
        };
        let parsed: Vec<(String, String)> = items
            .iter()
            .filter_map(|item| {
                let content = item.get("content")?.as_str()?.to_owned();
                let status = item.get("status")?.as_str()?.to_owned();
                matches!(status.as_str(), "pending" | "in_progress" | "completed")
                    .then_some((content, status))
            })
            .collect();
        if parsed.len() != items.len() {
            return Ok(tool_error_json(
                "every todo needs content + status (pending | in_progress | completed)",
            ));
        }
        let rendered: Vec<String> = parsed
            .iter()
            .map(|(content, status)| {
                let mark = match status.as_str() {
                    "completed" => "[x]",
                    "in_progress" => "[>]",
                    _ => "[ ]",
                };
                format!("{mark} {content}")
            })
            .collect();
        *self.todos.lock().expect("todo lock poisoned") = parsed;
        Ok(json!({ "todos": rendered }).to_string())
    }
}

/// Registers `todo_write` (one instance per catalog → per-session list).
pub fn register_todo_tool(catalog: &mut ToolCatalog) -> Result<(), RegentError> {
    catalog.register(definition(), Arc::new(TodoTool::default()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::contracts::DenyAll;

    #[tokio::test]
    async fn replaces_the_list_and_rejects_bad_status() {
        let mut catalog = ToolCatalog::new();
        register_todo_tool(&mut catalog).unwrap();
        let ctx = ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll));

        let args = json!({"todos": [
            {"content": "read the code", "status": "completed"},
            {"content": "write the fix", "status": "in_progress"},
            {"content": "run tests", "status": "pending"},
        ]});
        let out = catalog
            .dispatch("todo_write", &args.to_string(), &ctx)
            .await;
        assert!(out.contains("[x] read the code"), "{out}");
        assert!(out.contains("[>] write the fix"), "{out}");
        assert!(out.contains("[ ] run tests"), "{out}");

        let bad = json!({"todos": [{"content": "x", "status": "someday"}]});
        let out = catalog.dispatch("todo_write", &bad.to_string(), &ctx).await;
        assert!(out.contains("error"), "{out}");
    }
}

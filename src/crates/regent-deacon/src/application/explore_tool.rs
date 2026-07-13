//! The `explore` tool (gap T3): delegate codebase reconnaissance to a
//! read-only scout subagent (see `session_manager::explore`) instead of
//! reading many files into the calling session's own context.

use crate::application::session_manager::SessionManager;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use regent_tools::{ToolContext, ToolExecutor};
use serde_json::{Value, json};
use std::sync::Weak;

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "explore".into(),
        description: "Delegate codebase reconnaissance — 'where is X handled', 'how does Y \
             flow' — to a read-only scout agent that answers with conclusions and exact file \
             paths, instead of reading many files into your own context. Use it when the \
             answer needs looking through more than a couple of files."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "question": {"type": "string", "description": "The reconnaissance question, specific enough to answer."},
                "context": {"type": "string", "description": "Optional extra context: what you already know, paths to start from."}
            },
            "required": ["question"]
        }),
        toolset: "delegation".into(),
    }
}

/// Runs the scout via the live manager. `Weak` so the tool never keeps the
/// manager alive past shutdown.
pub struct ExploreTool {
    sessions: Weak<SessionManager>,
}

impl ExploreTool {
    #[must_use]
    pub fn new(sessions: Weak<SessionManager>) -> Self {
        Self { sessions }
    }
}

#[async_trait]
impl ToolExecutor for ExploreTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(question) = args.get("question").and_then(Value::as_str) else {
            return Ok(tool_error_json("missing required parameter: question"));
        };
        let context = args.get("context").and_then(Value::as_str);
        let Some(sessions) = self.sessions.upgrade() else {
            return Ok(tool_error_json("deacon is shutting down"));
        };
        match sessions.run_explore(question, context).await {
            Ok(answer) => Ok(json!({ "answer": answer }).to_string()),
            Err(error) => Ok(tool_error_json(format!("explore failed: {error}"))),
        }
    }
}

//! The `code_task` tool — automatic routing into the coding harness (ADR-027).
//!
//! Chat sessions carry this tool so the MODEL routes coding work to the
//! plan → execute → verify → revert-on-fail harness instead of freestyle
//! editing — no classifier, no extra model call: the routing decision rides
//! on the turn the model was already making. A re-entrancy flag stops the
//! harness's own execute session from recursing into another code_task.

use crate::application::session_manager::SessionManager;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use regent_tools::{ToolContext, ToolExecutor};
use serde_json::{Value, json};
use std::sync::Weak;
use std::sync::atomic::{AtomicBool, Ordering};

static CODE_TASK_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "code_task".into(),
        description: "Run a coding task through the coding harness: plan (read-only) → \
             execute → verify with the repo's tests/build → revert on failure. USE THIS for \
             any nontrivial code change — direct edits skip verification and can't roll \
             back. Skip for trivial one-liners or an explicit raw-edit ask. Pass the user's \
             request as `task` with any context."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "task": {"type": "string", "description": "The coding task, in full."},
                "skill": {"type": "string", "description": "Optional skill name; \
                     'ponytail' = minimal-diff work."}
            },
            "required": ["task"]
        }),
        toolset: "code".into(),
    }
}

/// Drives `code_plan` + `code_start` on the live manager. `Weak` so the tool
/// never keeps the manager alive past shutdown.
pub struct CodeTaskTool {
    sessions: Weak<SessionManager>,
}

impl CodeTaskTool {
    #[must_use]
    pub fn new(sessions: Weak<SessionManager>) -> Self {
        Self { sessions }
    }
}

#[async_trait]
impl ToolExecutor for CodeTaskTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(task) = args.get("task").and_then(Value::as_str) else {
            return Ok(tool_error_json("missing required parameter: task"));
        };
        let skill = args.get("skill").and_then(Value::as_str);
        let Some(sessions) = self.sessions.upgrade() else {
            return Ok(tool_error_json("deacon is shutting down"));
        };
        if CODE_TASK_IN_FLIGHT.swap(true, Ordering::SeqCst) {
            return Ok(tool_error_json(
                "a code task is already running — you are inside it; finish it directly",
            ));
        }
        let result = run(&sessions, task, skill).await;
        CODE_TASK_IN_FLIGHT.store(false, Ordering::SeqCst);
        Ok(result)
    }
}

async fn run(sessions: &SessionManager, task: &str, skill: Option<&str>) -> String {
    let (_plan_sid, plan) = match sessions.code_plan(task, skill).await {
        Ok(v) => v,
        Err(e) => return tool_error_json(format!("code.plan failed: {e}")),
    };
    match sessions.code_start(task, &plan, skill, &[]).await {
        Ok(outcome) => json!({
            "success": true,
            "plan": plan,
            "report": outcome.report,
            "verify": outcome.verify.as_ref().map(|v| json!({
                "passed": v.passed,
                "summary": v.summary,
            })),
            "fix_attempts": outcome.fix_attempts,
            "reverted": outcome.reverted,
        })
        .to_string(),
        Err(e) => tool_error_json(format!("code.start failed: {e}")),
    }
}

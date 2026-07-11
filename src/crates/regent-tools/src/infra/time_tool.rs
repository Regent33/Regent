//! `current_time` — the exact present moment, on demand. The system prompt's
//! date line is frozen at session build (prompt-cache stability), so a
//! long-lived session drifts; this tool is how the agent answers "what's the
//! exact date and time" truthfully at any point. Zero-parameter, tiny schema,
//! pinned (never auto-deferred).

use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_result_json};
use serde_json::{Value, json};

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "current_time".into(),
        description: "The user's current local date and time (plus UTC and unix epoch). Use \
                      whenever the exact present moment matters — today's date, elapsed time, \
                      deadlines — instead of the session-start date in your instructions."
            .into(),
        parameters: json!({"type": "object", "properties": {}}),
        toolset: "core".into(),
    }
}

pub struct CurrentTimeTool;

#[async_trait]
impl ToolExecutor for CurrentTimeTool {
    async fn execute(&self, _args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let local = chrono::Local::now();
        Ok(tool_result_json(json!({
            "local": local.format("%A, %B %e, %Y at %I:%M:%S %p (UTC%:z)").to_string(),
            "utc": local.with_timezone(&chrono::Utc).to_rfc3339(),
            "unix": local.timestamp(),
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::contracts::DenyAll;
    use std::sync::Arc;

    #[tokio::test]
    async fn reports_a_real_present_moment() {
        let ctx = ToolContext::new(std::path::PathBuf::from("."), Arc::new(DenyAll));
        let out = CurrentTimeTool.execute(json!({}), &ctx).await.unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        // A sane epoch (after 2025) and a formatted local string with a year.
        assert!(v["unix"].as_i64().unwrap() > 1_735_000_000, "{v}");
        assert!(v["local"].as_str().unwrap().contains("20"), "{v}");
        assert!(v["utc"].as_str().unwrap().contains('T'), "{v}");
    }
}

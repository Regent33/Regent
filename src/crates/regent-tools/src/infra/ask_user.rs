//! `ask_user` (gap T4) — one blocking structured question to the human. Rides
//! the existing approval channel end-to-end: the surface renders the question,
//! the reply comes back as the approval decision (`Approve` = plain yes, the
//! deny-feedback string = a free-text answer). No new RPC method needed.
// ponytail: an auto-approving surface (REGENT_AUTO_APPROVE voice deacon)
// answers every question "yes" — acceptable degenerate; a dedicated question
// channel only if that ever bites.

use crate::ToolCatalog;
use crate::domain::contracts::{ApprovalDecision, ToolExecutor};
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};
use std::sync::Arc;

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "ask_user".into(),
        description: "Ask the user ONE blocking question when you cannot proceed safely on \
             assumptions (missing requirement, ambiguous instruction, irreversible choice). \
             The turn waits for their reply."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "question": {"type": "string", "description": "The question, self-contained."},
                "context": {
                    "type": "string",
                    "description": "Why you're asking / the options, in one or two lines."
                }
            },
            "required": ["question"]
        }),
        toolset: "core".into(),
    }
}

struct AskUserTool;

#[async_trait]
impl ToolExecutor for AskUserTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(question) = args.get("question").and_then(Value::as_str) else {
            return Ok(tool_error_json("ask_user needs 'question' (a string)"));
        };
        let context = args
            .get("context")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match ctx.approval.request("ask_user", question, context).await {
            ApprovalDecision::Approve => Ok(json!({"answer": "yes"}).to_string()),
            ApprovalDecision::DenyWithFeedback(text) => Ok(json!({"answer": text}).to_string()),
            ApprovalDecision::Deny => Ok(tool_error_json(
                "no answer (declined or timed out) — proceed on your best judgment and state \
                 the assumption you made",
            )),
        }
    }
}

/// Registers `ask_user` (code sessions only — chat has the human in the loop).
pub fn register_ask_user_tool(catalog: &mut ToolCatalog) -> Result<(), RegentError> {
    catalog.register(definition(), Arc::new(AskUserTool))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::contracts::ApprovalHandler;

    struct Scripted(ApprovalDecision);

    #[async_trait]
    impl ApprovalHandler for Scripted {
        async fn request(&self, _: &str, _: &str, _: &str) -> ApprovalDecision {
            self.0.clone()
        }
    }

    async fn ask(decision: ApprovalDecision) -> String {
        let mut catalog = ToolCatalog::new();
        register_ask_user_tool(&mut catalog).unwrap();
        let ctx = ToolContext::new(std::env::temp_dir(), Arc::new(Scripted(decision)));
        catalog
            .dispatch(
                "ask_user",
                &json!({"question": "tabs or spaces?"}).to_string(),
                &ctx,
            )
            .await
    }

    #[tokio::test]
    async fn maps_each_decision_to_an_answer() {
        assert_eq!(ask(ApprovalDecision::Approve).await, r#"{"answer":"yes"}"#);
        assert_eq!(
            ask(ApprovalDecision::DenyWithFeedback("spaces, 2".into())).await,
            r#"{"answer":"spaces, 2"}"#
        );
        assert!(ask(ApprovalDecision::Deny).await.contains("error"));
    }
}

//! Gap S5/S6 acceptance: permission rules as data — a Deny rule's feedback
//! becomes the tool result (the model steers instead of stalling); Ask routes
//! through the surface's approval handler; no matching rule = old behavior.

use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition};
use regent_tools::{
    DenyAll, PermissionAction, PermissionRule, ToolCatalog, ToolContext, ToolExecutor,
};
use serde_json::{Value, json};
use std::sync::Arc;

struct Echo;

#[async_trait]
impl ToolExecutor for Echo {
    async fn execute(&self, _args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        Ok("\"ok\"".into())
    }
}

fn catalog() -> ToolCatalog {
    let mut catalog = ToolCatalog::new();
    catalog
        .register(
            ToolDefinition {
                name: "echo".into(),
                description: "test".into(),
                parameters: json!({"type": "object"}),
                toolset: "test".into(),
            },
            Arc::new(Echo),
        )
        .unwrap();
    catalog
}

#[tokio::test]
async fn permission_rules_deny_with_feedback_and_gate_ask() {
    let rules = vec![
        PermissionRule {
            permission: "echo".into(),
            pattern: "*.env*".into(),
            action: PermissionAction::Deny,
            feedback: Some("secrets stay sealed; ask the user directly".into()),
        },
        PermissionRule {
            permission: "echo".into(),
            pattern: "gated*".into(),
            action: PermissionAction::Ask,
            feedback: None,
        },
    ];
    let ctx =
        ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll)).with_permission_rules(rules);
    let catalog = catalog();

    // Deny → the feedback IS the result (the model can steer).
    let out = catalog
        .dispatch("echo", r#"{"path": "config/.env"}"#, &ctx)
        .await;
    assert!(out.contains("secrets stay sealed"), "{out}");

    // Ask → DenyAll approval → denied, tool never runs.
    let out = catalog
        .dispatch("echo", r#"{"path": "gated/file"}"#, &ctx)
        .await;
    assert!(out.contains("not approved"), "{out}");

    // No matching rule → dispatch proceeds as ever.
    let out = catalog
        .dispatch("echo", r#"{"path": "src/main.rs"}"#, &ctx)
        .await;
    assert_eq!(out, "\"ok\"");
}

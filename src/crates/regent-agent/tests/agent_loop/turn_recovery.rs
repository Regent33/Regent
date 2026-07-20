//! A failed tool call reveals deferred schemas before the next model call.

use crate::helpers::{call, test_context, text_response, tool_call_response};
use async_trait::async_trait;
use regent_agent::{Agent, AgentConfig};
use regent_kernel::{RegentError, Role, ToolDefinition, tool_error_json};
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
use regent_store::Store;
use regent_tools::{ToolCatalog, ToolContext, ToolExecutor};
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

struct AlwaysErrors;

#[async_trait]
impl ToolExecutor for AlwaysErrors {
    async fn execute(
        &self,
        _args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<String, RegentError> {
        Ok(tool_error_json("wrong visible tool"))
    }
}

struct DeferredRecoveryProvider {
    step: AtomicUsize,
}

#[async_trait]
impl ChatProvider for DeferredRecoveryProvider {
    async fn complete(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        let names: Vec<_> = request
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect();
        Ok(match self.step.fetch_add(1, Ordering::SeqCst) {
            0 => {
                assert!(!names.contains(&"read_document"));
                assert!(!names.contains(&"create_document"));
                tool_call_response(vec![call("wrong", "visible_tool", json!({}))])
            }
            1 => {
                assert!(names.contains(&"read_document"));
                assert!(names.contains(&"create_document"));
                text_response("document tools available")
            }
            _ => return Err(ProviderError::Parse("script exhausted".into())),
        })
    }

    fn model(&self) -> &str {
        "deferred-recovery-model"
    }
}

fn deferred_recovery_catalog() -> Arc<ToolCatalog> {
    let mut catalog = ToolCatalog::new();
    let definition = |name: &str| ToolDefinition {
        name: name.into(),
        description: "test tool".into(),
        parameters: json!({"type": "object"}),
        toolset: "test".into(),
    };
    catalog
        .register(definition("visible_tool"), Arc::new(AlwaysErrors))
        .unwrap();
    catalog
        .register(definition("read_document"), Arc::new(AlwaysErrors))
        .unwrap();
    catalog
        .register(definition("create_document"), Arc::new(AlwaysErrors))
        .unwrap();
    catalog
        .defer(&["read_document".into(), "create_document".into()])
        .unwrap();
    Arc::new(catalog)
}

#[tokio::test]
async fn tool_error_reveals_deferred_schemas_before_next_model_call() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let provider: Arc<dyn ChatProvider> = Arc::new(DeferredRecoveryProvider {
        step: AtomicUsize::new(0),
    });
    let mut agent = Agent::new(
        provider,
        deferred_recovery_catalog(),
        Arc::clone(&store),
        test_context(),
        "system",
        AgentConfig::default(),
    )
    .unwrap();

    let reply = agent.run_turn("read the attached document").await.unwrap();
    assert_eq!(reply, "document tools available");
    let rows = store.get_conversation(agent.session_id()).unwrap();
    assert_eq!(
        rows.iter().map(|row| row.message.role).collect::<Vec<_>>(),
        vec![Role::User, Role::Assistant, Role::Tool, Role::Assistant,]
    );
    assert!(
        rows[2]
            .message
            .content
            .as_deref()
            .unwrap()
            .contains("error")
    );
}

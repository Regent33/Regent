//! The context meter must count the tool catalog.
//!
//! Tool definitions do not live in the system prompt — they ride their own
//! `tools` array — so a meter built from `system_prompt + transcript` reported
//! a number the provider never saw. On the deacon's catalog that was ~11k
//! tokens missing from *every* turn: a bill charged and not shown, and
//! headroom compaction never reserved.

use async_trait::async_trait;
use or_core::TokenUsage;
use regent_agent::{Agent, AgentConfig};
use regent_kernel::{ChatMessage, ToolDefinition};
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
use regent_store::Store;
use regent_tools::{DenyAll, ToolCatalog, ToolContext, ToolExecutor};
use serde_json::json;
use std::sync::Arc;

struct Idle;

#[async_trait]
impl ChatProvider for Idle {
    async fn complete(&self, _request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        Ok(ChatResponse {
            message: ChatMessage::assistant(Some("ok".into()), vec![]),
            usage: TokenUsage::default(),
            finish_reason: Some("stop".into()),
        })
    }
    fn model(&self) -> &str {
        "claude-sonnet-4-5"
    }
}

struct Noop;

#[async_trait]
impl ToolExecutor for Noop {
    async fn execute(
        &self,
        _args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<String, regent_kernel::RegentError> {
        Ok("{}".into())
    }
}

/// A definition with a deliberately fat description + schema, so the cost is
/// unmistakable rather than rounding noise.
fn fat_tool(name: &str) -> ToolDefinition {
    ToolDefinition {
        name: name.to_owned(),
        description: "d".repeat(800),
        parameters: json!({
            "type": "object",
            "properties": { "q": { "type": "string", "description": "p".repeat(800) } }
        }),
        toolset: "test".to_owned(),
    }
}

fn agent_with(catalog: ToolCatalog) -> Agent {
    Agent::new(
        Arc::new(Idle),
        Arc::new(catalog),
        Arc::new(Store::open_in_memory().unwrap()),
        ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll)),
        "system",
        AgentConfig::default(),
    )
    .unwrap()
}

#[test]
fn an_empty_catalog_costs_nothing() {
    let agent = agent_with(ToolCatalog::new());
    assert_eq!(agent.tool_schema_tokens(), 0);
}

#[test]
fn the_meter_counts_the_tool_catalog() {
    let bare = agent_with(ToolCatalog::new()).context_usage().0;

    let mut catalog = ToolCatalog::new();
    for i in 0..10 {
        catalog
            .register(fat_tool(&format!("tool_{i}")), Arc::new(Noop))
            .unwrap();
    }
    let agent = agent_with(catalog);

    let schemas = agent.tool_schema_tokens();
    assert!(
        schemas > 3_000,
        "ten fat schemas should cost thousands of tokens, got {schemas}"
    );
    assert_eq!(
        agent.context_usage().0,
        bare + schemas,
        "the meter is the prompt+transcript estimate PLUS the catalog — the \
         catalog half is what it used to omit"
    );
}

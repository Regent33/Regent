//! Interrupt during tool dispatch: a cancel drops the in-flight tool future
//! (and any delegated children inside it), so a long-running tool never
//! completes and the turn returns `Interrupted`.

use async_trait::async_trait;
use or_core::TokenUsage;
use regent_agent::{Agent, AgentConfig};
use regent_kernel::{ChatMessage, RegentError, ToolCall, ToolDefinition};
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
use regent_store::Store;
use regent_tools::{ToolCatalog, ToolContext, ToolExecutor};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Always asks for the `slow` tool on the first model call.
struct CallsSlowTool;
#[async_trait]
impl ChatProvider for CallsSlowTool {
    async fn complete(&self, _request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        Ok(ChatResponse {
            message: ChatMessage::assistant(
                None,
                vec![ToolCall { id: "c1".into(), name: "slow".into(), arguments: "{}".into() }],
            ),
            usage: TokenUsage::default(),
            finish_reason: Some("tool_calls".into()),
        })
    }
    fn model(&self) -> &str {
        "calls-slow"
    }
}

/// Sleeps, then records that it finished — so the test can prove it was
/// dropped mid-run rather than allowed to complete.
struct SlowTool {
    finished: Arc<AtomicBool>,
}
#[async_trait]
impl ToolExecutor for SlowTool {
    async fn execute(&self, _args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        tokio::time::sleep(Duration::from_secs(10)).await;
        self.finished.store(true, Ordering::SeqCst);
        Ok("done".into())
    }
}

#[tokio::test]
async fn cancel_aborts_in_flight_tool_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let finished = Arc::new(AtomicBool::new(false));

    let mut catalog = ToolCatalog::new();
    let def = ToolDefinition {
        name: "slow".into(),
        description: "sleeps".into(),
        parameters: json!({"type": "object"}),
        toolset: "test".into(),
    };
    catalog.register(def, Arc::new(SlowTool { finished: Arc::clone(&finished) })).unwrap();

    let ctx = ToolContext::new(std::env::temp_dir(), Arc::new(regent_tools::DenyAll));
    let mut agent = Agent::new(
        Arc::new(CallsSlowTool),
        Arc::new(catalog),
        store,
        ctx,
        "system",
        AgentConfig::default(),
    )
    .unwrap();
    let cancel = agent.cancel_handle();

    let handle = tokio::spawn(async move { agent.run_turn("go").await });
    // Let the (instant) model call land and the slow tool start.
    tokio::time::sleep(Duration::from_millis(150)).await;
    cancel.cancel();

    let result = handle.await.unwrap();
    assert!(matches!(result, Err(RegentError::Interrupted)), "got {result:?}");
    assert!(!finished.load(Ordering::SeqCst), "slow tool was dropped mid-run, never completed");
}

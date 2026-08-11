//! Gap L1 acceptance: the third identical single-call batch in a row is not
//! dispatched — the model gets a synthetic steering result instead, and the
//! tool itself runs exactly twice.

use async_trait::async_trait;
use or_core::TokenUsage;
use regent_agent::{Agent, AgentConfig};
use regent_kernel::{ChatMessage, RegentError, Role, ToolCall, ToolDefinition};
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
use regent_store::Store;
use regent_tools::{DenyAll, ToolCatalog, ToolContext, ToolExecutor};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU32, Ordering},
};

struct ScriptedProvider(Mutex<VecDeque<ChatResponse>>);

#[async_trait]
impl ChatProvider for ScriptedProvider {
    async fn complete(&self, _request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        self.0
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| ProviderError::Parse("script exhausted".into()))
    }

    fn model(&self) -> &str {
        "scripted-model"
    }
}

struct CountingTool(Arc<AtomicU32>);

#[async_trait]
impl ToolExecutor for CountingTool {
    async fn execute(&self, _args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(json!({"ok": true}).to_string())
    }
}

fn same_call(id: &str) -> ChatResponse {
    ChatResponse {
        message: ChatMessage::assistant(
            None,
            vec![ToolCall {
                id: id.into(),
                name: "probe".into(),
                arguments: json!({"q": "same"}).to_string(),
            }],
        ),
        usage: TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            ..Default::default()
        },
        finish_reason: Some("tool_calls".into()),
    }
}

#[tokio::test]
async fn third_identical_call_is_nudged_not_dispatched() {
    let executions = Arc::new(AtomicU32::new(0));
    let mut catalog = ToolCatalog::new();
    catalog
        .register(
            ToolDefinition {
                name: "probe".into(),
                description: "test double".into(),
                parameters: json!({"type": "object"}),
                toolset: "test".into(),
            },
            Arc::new(CountingTool(Arc::clone(&executions))),
        )
        .unwrap();

    let store = Arc::new(Store::open_in_memory().unwrap());
    let provider = Arc::new(ScriptedProvider(Mutex::new(
        vec![
            same_call("a"),
            same_call("b"),
            same_call("c"), // 3rd identical → nudged, not dispatched
            same_call("d"), // still repeating → nudged again
            ChatResponse {
                message: ChatMessage::assistant(Some("gave up gracefully".into()), vec![]),
                usage: TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    total_tokens: 15,
                    ..Default::default()
                },
                finish_reason: Some("stop".into()),
            },
        ]
        .into(),
    )));
    let mut agent = Agent::new(
        provider,
        Arc::new(catalog),
        Arc::clone(&store),
        ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll)),
        "system",
        AgentConfig::default(),
    )
    .unwrap();

    let reply = agent.run_turn("go").await.unwrap();
    assert_eq!(reply, "gave up gracefully");
    assert_eq!(
        executions.load(Ordering::SeqCst),
        2,
        "only the first two identical calls actually execute"
    );

    // The 3rd and 4th tool results are the synthetic nudge, persisted like any
    // other result so a resumed session replays legally.
    let rows = store.get_conversation(agent.session_id()).unwrap();
    let tool_results: Vec<String> = rows
        .iter()
        .filter(|r| r.message.role == Role::Tool)
        .map(|r| r.message.content.clone().unwrap_or_default())
        .collect();
    assert_eq!(tool_results.len(), 4);
    assert!(tool_results[0].contains("ok"));
    assert!(tool_results[2].contains("3 times"), "{}", tool_results[2]);
    assert!(tool_results[3].contains("3 times"));
}

#[tokio::test]
async fn budget_wrap_up_is_included_in_live_and_persisted_turn_usage() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let provider = Arc::new(ScriptedProvider(Mutex::new(
        vec![
            same_call("budget-tool"),
            ChatResponse {
                message: ChatMessage::assistant(Some("budget summary".into()), vec![]),
                usage: TokenUsage {
                    prompt_tokens: 20,
                    completion_tokens: 8,
                    total_tokens: 28,
                    cache_read_tokens: Some(7),
                    cache_write_tokens: Some(2),
                },
                finish_reason: Some("stop".into()),
            },
        ]
        .into(),
    )));
    let config = AgentConfig {
        max_iterations: 1,
        ..AgentConfig::default()
    };
    let mut catalog = ToolCatalog::new();
    catalog
        .register(
            ToolDefinition {
                name: "probe".into(),
                description: "test double".into(),
                parameters: json!({"type": "object"}),
                toolset: "test".into(),
            },
            Arc::new(CountingTool(Arc::new(AtomicU32::new(0)))),
        )
        .unwrap();
    let mut agent = Agent::new(
        provider,
        Arc::new(catalog),
        Arc::clone(&store),
        ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll)),
        "system",
        config,
    )
    .unwrap();

    assert_eq!(
        agent.run_turn("finish safely").await.unwrap(),
        "budget summary"
    );
    assert_eq!(agent.last_turn_usage(), (30, 13));
    assert_eq!(agent.last_turn_cache_usage(), (Some(7), Some(2)));
    let persisted = store.insights().unwrap();
    assert_eq!((persisted.input_tokens, persisted.output_tokens), (30, 13));
    assert_eq!(persisted.api_calls, 2);
}

#[tokio::test]
async fn unreported_final_call_clears_the_observed_last_request_size() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let provider = Arc::new(ScriptedProvider(Mutex::new(
        vec![
            same_call("budget-tool"),
            ChatResponse {
                message: ChatMessage::assistant(Some("usage unavailable".into()), vec![]),
                // Anthropic's missing-usage shape still carries zero-valued
                // cache fields; those must not make absent input/output look
                // provider-reported.
                usage: TokenUsage {
                    cache_read_tokens: Some(0),
                    cache_write_tokens: Some(0),
                    ..TokenUsage::default()
                },
                finish_reason: Some("stop".into()),
            },
        ]
        .into(),
    )));
    let config = AgentConfig {
        max_iterations: 1,
        ..AgentConfig::default()
    };
    let mut catalog = ToolCatalog::new();
    catalog
        .register(
            ToolDefinition {
                name: "probe".into(),
                description: "test double".into(),
                parameters: json!({"type": "object"}),
                toolset: "test".into(),
            },
            Arc::new(CountingTool(Arc::new(AtomicU32::new(0)))),
        )
        .unwrap();
    let mut agent = Agent::new(
        provider,
        Arc::new(catalog),
        Arc::clone(&store),
        ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll)),
        "system",
        config,
    )
    .unwrap();

    assert_eq!(
        agent.run_turn("finish safely").await.unwrap(),
        "usage unavailable"
    );
    assert_eq!(agent.last_turn_usage(), (10, 5));
    assert!(!agent.last_turn_usage_complete());
    assert_eq!(agent.last_request_input_tokens(), None);
    let persisted = store.insights().unwrap();
    assert_eq!(persisted.api_calls, 2);
    assert_eq!(persisted.unreported_usage_calls, 1);
    assert!(!persisted.legacy_usage_unverified);
}

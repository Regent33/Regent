//! Gap L3 acceptance: read-only tool calls in a batch dispatch in parallel;
//! mutating calls dispatch serially, in call order; results re-attach in the
//! original call order regardless of partitioning.

use async_trait::async_trait;
use or_core::TokenUsage;
use regent_agent::{Agent, AgentConfig};
use regent_kernel::{ChatMessage, RegentError, Role, ToolCall, ToolDefinition};
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
use regent_store::Store;
use regent_tools::{DenyAll, ToolCatalog, ToolContext, ToolExecutor};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

/// Records `start:<n>` / `end:<n>` around a short sleep, so the event log
/// shows whether two executions overlapped.
struct TrackTool {
    events: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl ToolExecutor for TrackTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let n = args["n"].as_i64().unwrap_or(-1);
        self.events.lock().unwrap().push(format!("start:{n}"));
        tokio::time::sleep(Duration::from_millis(50)).await;
        self.events.lock().unwrap().push(format!("end:{n}"));
        Ok(json!({"done": n}).to_string())
    }
}

fn call(id: &str, name: &str, n: i64) -> ToolCall {
    ToolCall {
        id: id.into(),
        name: name.into(),
        arguments: json!({ "n": n }).to_string(),
    }
}

fn tool_call_response(calls: Vec<ToolCall>) -> ChatResponse {
    ChatResponse {
        message: ChatMessage::assistant(None, calls),
        usage: TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            ..Default::default()
        },
        finish_reason: Some("tool_calls".into()),
    }
}

fn text_response(text: &str) -> ChatResponse {
    ChatResponse {
        message: ChatMessage::assistant(Some(text.into()), vec![]),
        usage: TokenUsage {
            prompt_tokens: 20,
            completion_tokens: 8,
            total_tokens: 28,
            ..Default::default()
        },
        finish_reason: Some("stop".into()),
    }
}

/// Catalog with tracking executors registered under one read-only name
/// (`read_file`) and one mutating name (`file_edit`), sharing an event log.
fn tracking_catalog(events: &Arc<Mutex<Vec<String>>>) -> Arc<ToolCatalog> {
    let mut catalog = ToolCatalog::new();
    for name in ["read_file", "file_edit"] {
        catalog
            .register(
                ToolDefinition {
                    name: name.into(),
                    description: "tracking double".into(),
                    parameters: json!({"type": "object"}),
                    toolset: "test".into(),
                },
                Arc::new(TrackTool {
                    events: Arc::clone(events),
                }),
            )
            .unwrap();
    }
    Arc::new(catalog)
}

fn pos(events: &[String], needle: &str) -> usize {
    events
        .iter()
        .position(|e| e == needle)
        .unwrap_or_else(|| panic!("event {needle} missing from {events:?}"))
}

#[tokio::test]
async fn reads_overlap_edits_serialize_results_keep_call_order() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let store = Arc::new(Store::open_in_memory().unwrap());
    let provider = Arc::new(ScriptedProvider(Mutex::new(
        vec![
            tool_call_response(vec![
                call("a", "read_file", 1),
                call("b", "read_file", 2),
                call("c", "file_edit", 3),
                call("d", "read_file", 4),
                call("e", "file_edit", 5),
            ]),
            text_response("done"),
        ]
        .into(),
    )));
    let mut agent = Agent::new(
        provider,
        tracking_catalog(&events),
        Arc::clone(&store),
        ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll)),
        "system",
        AgentConfig::default(),
    )
    .unwrap();

    assert_eq!(agent.run_turn("go").await.unwrap(), "done");

    let events = events.lock().unwrap().clone();
    // The two leading reads ran as one parallel batch: both started before
    // either finished.
    assert!(pos(&events, "start:2") < pos(&events, "end:1").min(pos(&events, "end:2")));
    // The edit run waited for the read batch, and later runs stayed ordered.
    assert!(pos(&events, "start:3") > pos(&events, "end:1").max(pos(&events, "end:2")));
    assert!(pos(&events, "end:3") < pos(&events, "start:4"));
    assert!(pos(&events, "end:4") < pos(&events, "start:5"));

    // Results re-attached in original call order.
    let rows = store.get_conversation(agent.session_id()).unwrap();
    let tool_ids: Vec<_> = rows
        .iter()
        .filter(|r| r.message.role == Role::Tool)
        .map(|r| r.message.tool_call_id.clone().unwrap())
        .collect();
    assert_eq!(tool_ids, vec!["a", "b", "c", "d", "e"]);
}

#[tokio::test]
async fn mutating_batch_never_interleaves() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let store = Arc::new(Store::open_in_memory().unwrap());
    let provider = Arc::new(ScriptedProvider(Mutex::new(
        vec![
            tool_call_response(vec![
                call("a", "file_edit", 1),
                call("b", "file_edit", 2),
                call("c", "file_edit", 3),
            ]),
            text_response("done"),
        ]
        .into(),
    )));
    let mut agent = Agent::new(
        provider,
        tracking_catalog(&events),
        store,
        ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll)),
        "system",
        AgentConfig::default(),
    )
    .unwrap();

    agent.run_turn("go").await.unwrap();

    let events = events.lock().unwrap().clone();
    assert_eq!(
        events,
        vec!["start:1", "end:1", "start:2", "end:2", "start:3", "end:3"],
        "mutating calls must run strictly one after another"
    );
}

//! M2 integration: the memory tool writing THROUGH a live turn while the
//! cached system prompt stays byte-identical (the frozen-snapshot
//! invariant), and episode capture on compression splits.

use async_trait::async_trait;
use or_core::TokenUsage;
use regent_agent::{Agent, AgentConfig};
use regent_graph::{GraphMemory, MemoryTarget};
use regent_kernel::{ChatMessage, ToolCall};
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
use regent_store::Store;
use regent_tools::{DenyAll, ToolCatalog, ToolContext, register_memory_tools};
use serde_json::json;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Scripted provider that also captures every request's system prompt.
struct CapturingProvider {
    responses: Mutex<VecDeque<ChatResponse>>,
    pub systems: Mutex<Vec<String>>,
}

impl CapturingProvider {
    fn new(responses: Vec<ChatResponse>) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(responses.into()),
            systems: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait]
impl ChatProvider for CapturingProvider {
    async fn complete(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        self.systems.lock().unwrap().push(request.system.clone());
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| ProviderError::Parse("script exhausted".into()))
    }

    fn model(&self) -> &str {
        "capturing-model"
    }
}

fn text_response(text: &str) -> ChatResponse {
    ChatResponse {
        message: ChatMessage::assistant(Some(text.into()), vec![]),
        usage: TokenUsage::default(),
        finish_reason: Some("stop".into()),
    }
}

fn memory_add_response(content: &str) -> ChatResponse {
    let call = ToolCall {
        id: "m1".into(),
        name: "memory".into(),
        arguments: json!({"action": "add", "target": "memory", "content": content}).to_string(),
    };
    ChatResponse {
        message: ChatMessage::assistant(None, vec![call]),
        usage: TokenUsage::default(),
        finish_reason: Some("tool_calls".into()),
    }
}

fn context() -> ToolContext {
    ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll))
}

#[tokio::test]
async fn memory_writes_mid_turn_never_mutate_the_cached_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let graph = Arc::new(GraphMemory::new(Arc::clone(&store)));

    let mut catalog = ToolCatalog::new();
    register_memory_tools(&mut catalog, Arc::clone(&graph), Arc::clone(&store)).unwrap();

    // Snapshot rendered at session start — empty stores.
    let system_prompt = format!("identity\n\n{}", graph.render_prompt_block().unwrap());
    assert!(system_prompt.contains("[0% — 0/2200 chars]"));

    let provider = CapturingProvider::new(vec![
        memory_add_response("User prefers Rust and tabs"),
        text_response("noted"),
        text_response("second turn answer"),
    ]);
    let mut agent = Agent::new(
        provider.clone(),
        Arc::new(catalog),
        Arc::clone(&store),
        context(),
        system_prompt.clone(),
        AgentConfig::default(),
    )
    .unwrap();

    agent
        .run_turn("remember that I prefer Rust and tabs")
        .await
        .unwrap();
    agent.run_turn("anything else?").await.unwrap();

    // The write landed immediately…
    let entries = graph.entries(MemoryTarget::Memory).unwrap();
    assert_eq!(entries, vec!["User prefers Rust and tabs".to_owned()]);

    // …but every API call in this session saw byte-identical system bytes.
    let systems = provider.systems.lock().unwrap();
    assert_eq!(systems.len(), 3);
    assert!(
        systems.iter().all(|s| *s == system_prompt),
        "cached prefix must never change"
    );

    // The NEXT session's snapshot includes the new entry.
    let next_prompt = format!("identity\n\n{}", graph.render_prompt_block().unwrap());
    assert!(next_prompt.contains("User prefers Rust and tabs"));
    assert_ne!(next_prompt, system_prompt);
}

#[tokio::test]
async fn compression_records_an_episode_node_for_the_parent_session() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let graph = Arc::new(GraphMemory::new(Arc::clone(&store)));
    let big = "x".repeat(400);

    let provider = CapturingProvider::new(vec![
        text_response("answer one"),
        text_response("answer two"),
        text_response("SUMMARY: migrated the database"),
        text_response("final answer"),
    ]);
    let config = AgentConfig {
        max_context_tokens: 500,
        compression: regent_agent::CompressionConfig {
            protect_last_n: 2,
            ..Default::default()
        },
        ..AgentConfig::default()
    };
    let mut agent = Agent::new(
        provider,
        Arc::new(ToolCatalog::new()),
        Arc::clone(&store),
        context(),
        "system",
        config,
    )
    .unwrap()
    .with_graph_memory(Arc::clone(&graph));

    agent.run_turn(&big).await.unwrap();
    agent.run_turn(&big).await.unwrap();
    let original = agent.session_id().clone();
    agent.run_turn(&big).await.unwrap();
    assert_ne!(agent.session_id(), &original);

    let episodes = store.nodes_by_kind("episode").unwrap();
    assert_eq!(episodes.len(), 1);
    assert!(
        episodes[0]
            .content
            .contains("SUMMARY: migrated the database")
    );
    assert_eq!(episodes[0].session_id.as_deref(), Some(original.as_str()));

    // And the episode is recallable through hybrid retrieval.
    let recalled = graph.retrieve("database migration", 5).unwrap();
    assert!(recalled.iter().any(|r| r.node.kind == "episode"));
}

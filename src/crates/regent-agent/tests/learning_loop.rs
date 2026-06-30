//! M3 learning-loop integration: the background review fork persists
//! memory without touching the main conversation, and an agent-created
//! skill survives into the next session's library (proposal M3 exit
//! criteria).

use async_trait::async_trait;
use or_core::TokenUsage;
use regent_agent::{Agent, AgentConfig, ReviewSetup};
use regent_graph::{GraphMemory, MemoryTarget};
use regent_kernel::{ChatMessage, ToolCall};
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
use regent_skills::{FsSkillRepository, REVIEW_SYSTEM_PROMPT, SkillLibrary};
use regent_store::Store;
use regent_tools::{
    DenyAll, ToolCatalog, ToolContext, register_memory_tools, register_skill_tools,
};
use serde_json::json;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

struct Scripted {
    responses: Mutex<VecDeque<ChatResponse>>,
}

impl Scripted {
    fn new(responses: Vec<ChatResponse>) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(responses.into()),
        })
    }
}

#[async_trait]
impl ChatProvider for Scripted {
    async fn complete(&self, _request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| ProviderError::Parse("script exhausted".into()))
    }

    fn model(&self) -> &str {
        "scripted-model"
    }
}

fn text(content: &str) -> ChatResponse {
    ChatResponse {
        message: ChatMessage::assistant(Some(content.into()), vec![]),
        usage: TokenUsage::default(),
        finish_reason: Some("stop".into()),
    }
}

fn tool_call(name: &str, args: serde_json::Value) -> ChatResponse {
    let call = ToolCall {
        id: "c1".into(),
        name: name.into(),
        arguments: args.to_string(),
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
async fn background_review_persists_memory_without_touching_the_conversation() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let graph = Arc::new(GraphMemory::new(Arc::clone(&store)));

    // Reviewer whitelist: memory tools only. Main catalog: empty.
    let mut review_catalog = ToolCatalog::new();
    register_memory_tools(&mut review_catalog, Arc::clone(&graph), Arc::clone(&store)).unwrap();

    // Script order: main turn answer, then the reviewer's two responses.
    let provider = Scripted::new(vec![
        text("the answer is 42"),
        tool_call(
            "memory",
            json!({"action": "add", "target": "user",
                                   "content": "User prefers concise answers"}),
        ),
        text("Nothing to save."),
    ]);

    let mut agent = Agent::new(
        provider,
        Arc::new(ToolCatalog::new()),
        Arc::clone(&store),
        context(),
        "main system prompt",
        AgentConfig::default(),
    )
    .unwrap()
    .with_background_review(ReviewSetup {
        catalog: Arc::new(review_catalog),
        system_prompt: REVIEW_SYSTEM_PROMPT.to_owned(),
        max_iterations: 8,
    });

    let reply = agent.run_turn("answer briefly: what is 6*7").await.unwrap();
    assert_eq!(reply, "the answer is 42");

    // The fork runs detached; await it deterministically for the test.
    agent.take_review_handle().unwrap().await.unwrap();

    // Learning landed…
    let entries = graph.entries(MemoryTarget::User).unwrap();
    assert_eq!(entries, vec!["User prefers concise answers".to_owned()]);
    // …and the main conversation was never touched (user + assistant only).
    let rows = store.get_conversation(agent.session_id()).unwrap();
    assert_eq!(rows.len(), 2);
}

#[tokio::test]
async fn agent_created_skill_persists_and_loads_next_session() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let skills_root = dir.path().join("skills");
    let library = Arc::new(SkillLibrary::new(Arc::new(
        FsSkillRepository::new(&skills_root).unwrap(),
    )));

    let mut catalog = ToolCatalog::new();
    register_skill_tools(&mut catalog, Arc::clone(&library)).unwrap();

    let provider = Scripted::new(vec![
        tool_call(
            "skill_manage",
            json!({
                "action": "create", "name": "release-checklist",
                "description": "Release checklist for the api service.",
                "body": "# Steps\n1. tag\n2. build\n3. announce"
            }),
        ),
        text("skill saved"),
    ]);
    let mut agent = Agent::new(
        provider,
        Arc::new(catalog),
        store,
        context(),
        "system",
        AgentConfig::default(),
    )
    .unwrap();
    assert_eq!(
        agent
            .run_turn("save what we learned as a skill")
            .await
            .unwrap(),
        "skill saved"
    );

    // "Next session": a fresh library over the same root sees the skill and
    // serves it through every disclosure level.
    let next_session_library =
        SkillLibrary::new(Arc::new(FsSkillRepository::new(&skills_root).unwrap()));
    let index = next_session_library.render_index().unwrap();
    assert!(index.contains("- release-checklist: Release checklist for the api service."));
    let record = next_session_library.view("release-checklist").unwrap();
    assert!(record.body.contains("announce"));
    assert_eq!(record.meta.created_by, "agent");
}

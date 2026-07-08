//! E2E agent-loop behavior with a scripted provider, a real tool catalog,
//! and a real on-disk store: prompt → parallel tool calls → final answer,
//! plus the harness stop conditions (budget, interrupt) and session resume.

use async_trait::async_trait;
use or_core::TokenUsage;
use regent_agent::{Agent, AgentConfig};
use regent_kernel::{ChatMessage, RegentError, Role, SessionId, ToolCall, ToolDefinition};
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
use regent_store::Store;
use regent_tools::{DenyAll, ToolCatalog, ToolContext, ToolExecutor};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

struct ScriptedProvider {
    responses: Mutex<VecDeque<ChatResponse>>,
    repeat_tool_calls_forever: bool,
    delay: Option<std::time::Duration>,
}

impl ScriptedProvider {
    fn scripted(responses: Vec<ChatResponse>) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(responses.into()),
            repeat_tool_calls_forever: false,
            delay: None,
        })
    }

    fn runaway() -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(VecDeque::new()),
            repeat_tool_calls_forever: true,
            delay: None,
        })
    }

    fn slow(delay: std::time::Duration) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(VecDeque::new()),
            repeat_tool_calls_forever: true,
            delay: Some(delay),
        })
    }
}

#[async_trait]
impl ChatProvider for ScriptedProvider {
    async fn complete(&self, _request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        if let Some(delay) = self.delay {
            tokio::time::sleep(delay).await;
        }
        if self.repeat_tool_calls_forever {
            return Ok(tool_call_response(vec![call(
                "loop",
                "echo",
                json!({"text": "again"}),
            )]));
        }
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

struct EchoTool;

#[async_trait]
impl ToolExecutor for EchoTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        Ok(json!({"echo": args["text"]}).to_string())
    }
}

fn call(id: &str, name: &str, args: Value) -> ToolCall {
    ToolCall {
        id: id.into(),
        name: name.into(),
        arguments: args.to_string(),
    }
}

fn tool_call_response(calls: Vec<ToolCall>) -> ChatResponse {
    ChatResponse {
        message: ChatMessage::assistant(None, calls),
        usage: TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
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
        },
        finish_reason: Some("stop".into()),
    }
}

fn echo_catalog() -> Arc<ToolCatalog> {
    let mut catalog = ToolCatalog::new();
    catalog
        .register(
            ToolDefinition {
                name: "echo".into(),
                description: "echo back".into(),
                parameters: json!({"type": "object"}),
                toolset: "test".into(),
            },
            Arc::new(EchoTool),
        )
        .unwrap();
    Arc::new(catalog)
}

fn test_context() -> ToolContext {
    ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll))
}

#[tokio::test]
async fn tool_round_trip_turn_persists_everything_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let provider = ScriptedProvider::scripted(vec![
        tool_call_response(vec![
            call("a", "echo", json!({"text": "one"})),
            call("b", "echo", json!({"text": "two"})),
        ]),
        text_response("all done"),
    ]);
    let mut agent = Agent::new(
        provider,
        echo_catalog(),
        Arc::clone(&store),
        test_context(),
        "system",
        AgentConfig::default(),
    )
    .unwrap();

    let reply = agent.run_turn("run the echoes").await.unwrap();
    assert_eq!(reply, "all done");

    let rows = store.get_conversation(agent.session_id()).unwrap();
    let roles: Vec<Role> = rows.iter().map(|r| r.message.role).collect();
    assert_eq!(
        roles,
        vec![
            Role::User,
            Role::Assistant,
            Role::Tool,
            Role::Tool,
            Role::Assistant
        ]
    );
    // results re-attached in original call order
    assert_eq!(rows[2].message.tool_call_id.as_deref(), Some("a"));
    assert!(rows[2].message.content.as_deref().unwrap().contains("one"));
    assert_eq!(rows[3].message.tool_call_id.as_deref(), Some("b"));
}

#[tokio::test]
async fn budget_ceiling_stops_runaway_loops() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let config = AgentConfig {
        max_iterations: 3,
        ..AgentConfig::default()
    };
    let mut agent = Agent::new(
        ScriptedProvider::runaway(),
        echo_catalog(),
        store,
        test_context(),
        "system",
        config,
    )
    .unwrap();

    let error = agent.run_turn("go").await.unwrap_err();
    assert!(matches!(error, RegentError::BudgetExhausted(3)));
}

#[tokio::test]
async fn token_ceiling_halts_the_turn_before_max_iterations() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    // Each runaway call spends 15 tokens (prompt 10 + completion 5). A 20-token
    // ceiling admits the first two calls (running total 0, then 15) and halts on
    // the third (30 ≥ 20) — well before the 90-iteration default ceiling. Proves
    // the per-turn token cap bounds spend independently of the step count (W2.4).
    let config = AgentConfig {
        max_turn_tokens: Some(20),
        ..AgentConfig::default()
    };
    let mut agent = Agent::new(
        ScriptedProvider::runaway(),
        echo_catalog(),
        store,
        test_context(),
        "system",
        config,
    )
    .unwrap();

    let error = agent.run_turn("go").await.unwrap_err();
    assert!(
        matches!(error, RegentError::BudgetExhausted(2)),
        "token ceiling should halt after 2 calls (30 tokens), got {error:?}"
    );
}

#[tokio::test]
async fn pre_cancelled_turn_is_interrupted_before_any_model_call() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let provider = ScriptedProvider::scripted(vec![text_response("never seen")]);
    let mut agent = Agent::new(
        provider,
        echo_catalog(),
        store,
        test_context(),
        "system",
        AgentConfig::default(),
    )
    .unwrap();
    agent.cancel_handle().cancel();

    let error = agent.run_turn("hello").await.unwrap_err();
    assert!(matches!(error, RegentError::Interrupted));
}

#[tokio::test]
async fn mid_call_interrupt_abandons_the_model_call_cleanly() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let provider = ScriptedProvider::slow(std::time::Duration::from_secs(30));
    let mut agent = Agent::new(
        provider,
        echo_catalog(),
        Arc::clone(&store),
        test_context(),
        "system",
        AgentConfig::default(),
    )
    .unwrap();
    let session = agent.session_id().clone();
    let cancel = agent.cancel_handle();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        cancel.cancel();
    });

    let error = agent.run_turn("hello").await.unwrap_err();
    assert!(matches!(error, RegentError::Interrupted));
    // Only the user message persisted — no partial assistant entered history.
    let rows = store.get_conversation(&session).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].message.role, Role::User);
    // The interrupted turn is still in the reproducibility ledger.
    let turns = store.turns_for_session(&session).unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].outcome, "interrupted");
}

#[tokio::test]
async fn turns_ledger_records_outcome_and_call_count() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let provider = ScriptedProvider::scripted(vec![
        tool_call_response(vec![call("a", "echo", json!({"text": "x"}))]),
        text_response("done"),
    ]);
    let mut agent = Agent::new(
        provider,
        echo_catalog(),
        Arc::clone(&store),
        test_context(),
        "system",
        AgentConfig::default(),
    )
    .unwrap();
    agent.run_turn("go").await.unwrap();

    let turns = store.turns_for_session(agent.session_id()).unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].outcome, "ok");
    assert_eq!(turns[0].api_calls, 2);
    assert_eq!(turns[0].model.as_deref(), Some("scripted-model"));
    assert!(turns[0].ended_at >= turns[0].started_at);
}

#[tokio::test]
async fn compression_splits_into_child_session_with_lineage() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let big = "x".repeat(400);

    // Seed two ordinary turns to build compressible history.
    let provider = ScriptedProvider::scripted(vec![
        text_response("answer one"),
        text_response("answer two"),
        // Turn 3 preflight triggers compression: first scripted response is
        // consumed by the summarizer, the second finishes the turn.
        text_response("SUMMARY OF EARLIER WORK"),
        text_response("final answer"),
    ]);
    // Estimates (chars/4, +16/message): turn-2 preflight ≈ 216 tokens stays
    // under the 250 threshold; turn-3 preflight ≈ 326 crosses it.
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
        echo_catalog(),
        Arc::clone(&store),
        test_context(),
        "system",
        config,
    )
    .unwrap();

    agent.run_turn(&big).await.unwrap();
    agent.run_turn(&big).await.unwrap();
    let original = agent.session_id().clone();
    let reply = agent.run_turn(&big).await.unwrap();
    assert_eq!(reply, "final answer");

    // Session split happened: new id, lineage back-pointer, old one ended.
    let child = agent.session_id().clone();
    assert_ne!(child, original);
    let child_meta = store.session_meta(&child).unwrap();
    assert_eq!(
        child_meta.parent_session_id.as_deref(),
        Some(original.as_str())
    );
    assert_eq!(child_meta.system_prompt.as_deref(), Some("system"));
    let parent_meta = store.session_meta(&original).unwrap();
    assert_eq!(parent_meta.end_reason.as_deref(), Some("compressed"));

    // Child history: summary first, protected tail verbatim, then the
    // continuing turn — and it must replay cleanly through resume.
    let rows = store.get_conversation(&child).unwrap();
    assert!(
        rows[0]
            .message
            .content
            .as_deref()
            .unwrap()
            .contains("SUMMARY OF EARLIER WORK")
    );
    assert!(
        rows.iter()
            .any(|r| r.message.content.as_deref() == Some(big.as_str()))
    );
    assert_eq!(
        rows.last().unwrap().message.content.as_deref(),
        Some("final answer")
    );
    let resumed = Agent::resume(
        ScriptedProvider::scripted(vec![]),
        echo_catalog(),
        Arc::clone(&store),
        test_context(),
        "ignored — stored prompt wins",
        AgentConfig::default(),
        child,
    );
    assert!(resumed.is_ok());
}

#[tokio::test]
async fn resume_repairs_history_left_by_failed_turns() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let session_id: SessionId;
    {
        // Turn 1 succeeds; turn 2 fails AFTER its user row is persisted
        // (script exhausted), leaving user,user corruption in the store once
        // turn 3 lands: user, assistant, user(dangling), user, assistant.
        let provider = ScriptedProvider::scripted(vec![
            text_response("first answer"),
            // nothing for turn 2 → provider error mid-turn
        ]);
        let mut agent = Agent::new(
            provider,
            echo_catalog(),
            Arc::clone(&store),
            test_context(),
            "system",
            AgentConfig::default(),
        )
        .unwrap();
        agent.run_turn("first question").await.unwrap();
        assert!(agent.run_turn("doomed question").await.is_err());
        session_id = agent.session_id().clone();
    }
    // The dangling user row is in the store; resume must repair, not brick,
    // and the next turn must not hit "two user messages in a row".
    let provider = ScriptedProvider::scripted(vec![text_response("recovered answer")]);
    let mut resumed = Agent::resume(
        provider,
        echo_catalog(),
        Arc::clone(&store),
        test_context(),
        "system",
        AgentConfig::default(),
        session_id.clone(),
    )
    .unwrap();
    let reply = resumed.run_turn("retry question").await.unwrap();
    assert_eq!(reply, "recovered answer");

    // The store now holds mid-history user,user; a second resume must also
    // replay cleanly and keep taking turns.
    let provider = ScriptedProvider::scripted(vec![text_response("still fine")]);
    let mut again = Agent::resume(
        provider,
        echo_catalog(),
        Arc::clone(&store),
        test_context(),
        "system",
        AgentConfig::default(),
        session_id,
    )
    .unwrap();
    assert_eq!(again.run_turn("one more").await.unwrap(), "still fine");
}

#[tokio::test]
async fn resume_replays_history_through_invariant_checks() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let session_id: SessionId;
    {
        let provider = ScriptedProvider::scripted(vec![text_response("first answer")]);
        let mut agent = Agent::new(
            provider,
            echo_catalog(),
            Arc::clone(&store),
            test_context(),
            "system",
            AgentConfig::default(),
        )
        .unwrap();
        agent.run_turn("first question").await.unwrap();
        session_id = agent.session_id().clone();
    }

    let provider = ScriptedProvider::scripted(vec![text_response("second answer")]);
    let mut resumed = Agent::resume(
        provider,
        echo_catalog(),
        Arc::clone(&store),
        test_context(),
        "system",
        AgentConfig::default(),
        session_id.clone(),
    )
    .unwrap();
    let reply = resumed.run_turn("second question").await.unwrap();
    assert_eq!(reply, "second answer");
    assert_eq!(store.get_conversation(&session_id).unwrap().len(), 4);
}

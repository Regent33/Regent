//! Happy-path turns, budget/token ceilings, and the turns ledger.

use crate::helpers::{
    ScriptedProvider, call, echo_catalog, test_context, text_response, tool_call_response,
};
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

// An empty completion (no text, no tool calls) is retried once — the user
// never sees a dead bubble. Persisted history holds no empty assistant row.
#[tokio::test]
async fn empty_response_retries_once_then_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let provider = ScriptedProvider::scripted(vec![
        text_response("   "), // whitespace-only = empty
        text_response("real answer"),
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

    let reply = agent.run_turn("hi").await.unwrap();
    assert_eq!(reply, "real answer");
    let rows = store.get_conversation(agent.session_id()).unwrap();
    assert!(
        rows.iter().all(|r| {
            r.message.role != Role::Assistant
                || r.message
                    .content
                    .as_deref()
                    .is_some_and(|c| !c.trim().is_empty())
        }),
        "no empty assistant row is ever persisted"
    );
}

// A model that only narrates its plan must not expose that reasoning as the
// answer. The harness retries privately, then executes the recovered tool call.
#[tokio::test]
async fn reasoning_only_plan_is_hidden_and_retried_into_tool_execution() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let mut reasoning_only = text_response("");
    reasoning_only.message.reasoning =
        Some("I should call echo, but first I will describe that plan.".into());
    let provider = ScriptedProvider::scripted(vec![
        reasoning_only,
        tool_call_response(vec![call("a", "echo", json!({"text": "recovered"}))]),
        text_response("tool completed"),
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

    let reply = agent.run_turn("use the echo tool").await.unwrap();
    assert_eq!(reply, "tool completed");

    let rows = store.get_conversation(agent.session_id()).unwrap();
    let roles: Vec<Role> = rows.iter().map(|row| row.message.role).collect();
    assert_eq!(
        roles,
        vec![Role::User, Role::Assistant, Role::Tool, Role::Assistant]
    );
    assert!(
        rows.iter().all(|row| row.message.reasoning.is_none()),
        "reasoning-only plan must never be persisted"
    );
    assert!(
        rows[2]
            .message
            .content
            .as_deref()
            .is_some_and(|content| content.contains("recovered")),
        "the recovered tool call must execute"
    );
    let turns = store.turns_for_session(agent.session_id()).unwrap();
    assert_eq!(turns[0].api_calls, 3);
}

// Some OpenAI-compatible models imitate Regent's tool syntax in visible text
// rather than returning a structured call. The fake call is discarded and a
// single private repair gets the model back onto the native tool path.
#[tokio::test]
async fn textual_pseudo_tool_call_is_hidden_and_retried_into_native_execution() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let provider = ScriptedProvider::scripted(vec![
        text_response("[echo: {\"text\":\"fabricated\"}]"),
        tool_call_response(vec![call("a", "echo", json!({"text": "real"}))]),
        text_response("tool completed"),
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

    let reply = agent.run_turn("use echo").await.unwrap();
    assert_eq!(reply, "tool completed");
    let rows = store.get_conversation(agent.session_id()).unwrap();
    assert_eq!(
        rows.iter().map(|row| row.message.role).collect::<Vec<_>>(),
        vec![Role::User, Role::Assistant, Role::Tool, Role::Assistant]
    );
    assert!(rows.iter().all(|row| {
        !row.message
            .content
            .as_deref()
            .is_some_and(|content| content.contains("fabricated"))
    }));
}

// A second empty completion is a loud provider error, never a silent Ok("").
#[tokio::test]
async fn twice_empty_response_is_a_provider_error() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let provider = ScriptedProvider::scripted(vec![text_response(""), text_response("")]);
    let mut agent = Agent::new(
        provider,
        echo_catalog(),
        store,
        test_context(),
        "system",
        AgentConfig::default(),
    )
    .unwrap();

    let err = agent.run_turn("hi").await.unwrap_err();
    assert!(err.to_string().contains("empty response"), "got: {err}");
}

// Gap L2: budget exhaustion is a graceful wrap-up, not a hard error — the
// turn returns Ok(summary) while the ledger still records `budget_exhausted`.
#[tokio::test]
async fn budget_ceiling_wraps_up_runaway_loops() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let config = AgentConfig {
        max_iterations: 3,
        ..AgentConfig::default()
    };
    let mut agent = Agent::new(
        ScriptedProvider::runaway(),
        echo_catalog(),
        Arc::clone(&store),
        test_context(),
        "system",
        config,
    )
    .unwrap();

    // The runaway provider answers the wrap-up call with another tool-call
    // response (no text) — the fallback summary still comes back as Ok, the
    // stray tool calls are dropped, and the transcript stays legal.
    let reply = agent.run_turn("go").await.unwrap();
    assert!(reply.contains("budget exhausted"), "got: {reply}");
    let turns = store.turns_for_session(agent.session_id()).unwrap();
    assert_eq!(turns[0].outcome, "budget_exhausted");
    // 3 working calls + 1 wrap-up call.
    assert_eq!(turns[0].api_calls, 4);
}

#[tokio::test]
async fn budget_wrap_up_returns_the_models_summary() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let config = AgentConfig {
        max_iterations: 2,
        ..AgentConfig::default()
    };
    let provider = ScriptedProvider::scripted(vec![
        tool_call_response(vec![call("a", "echo", json!({"text": "1"}))]),
        tool_call_response(vec![call("b", "echo", json!({"text": "2"}))]),
        // This response answers the tool-less wrap-up call.
        text_response("Done: X. Remaining: Y. Resume at Z."),
    ]);
    let mut agent = Agent::new(
        provider,
        echo_catalog(),
        Arc::clone(&store),
        test_context(),
        "system",
        config,
    )
    .unwrap();

    let reply = agent.run_turn("go").await.unwrap();
    assert_eq!(reply, "Done: X. Remaining: Y. Resume at Z.");
    let turns = store.turns_for_session(agent.session_id()).unwrap();
    assert_eq!(turns[0].outcome, "budget_exhausted");
}

#[tokio::test]
async fn token_ceiling_halts_the_turn_before_max_iterations() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    // Each runaway call spends 15 tokens (prompt 10 + completion 5). A 20-token
    // ceiling admits the first two calls (running total 0, then 15) and wraps
    // up on the third (30 ≥ 20) — well before the 90-iteration default ceiling.
    // Proves the per-turn token cap bounds spend independently of the step
    // count (W2.4); the ledger carries the exhaustion either way.
    let config = AgentConfig {
        max_turn_tokens: Some(20),
        ..AgentConfig::default()
    };
    let mut agent = Agent::new(
        ScriptedProvider::runaway(),
        echo_catalog(),
        Arc::clone(&store),
        test_context(),
        "system",
        config,
    )
    .unwrap();

    agent.run_turn("go").await.unwrap();
    let turns = store.turns_for_session(agent.session_id()).unwrap();
    assert_eq!(turns[0].outcome, "budget_exhausted");
    // 2 working calls before the ceiling + 1 wrap-up call.
    assert_eq!(
        turns[0].api_calls, 3,
        "token ceiling should halt after 2 working calls (30 tokens)"
    );
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

//! Request-local repairs: empty completions, reasoning-only plans, and
//! textual pseudo tool calls are retried privately — never persisted, never
//! shown to the user.

use crate::helpers::{ScriptedProvider, call, echo_catalog, test_context, text_response, tool_call_response};
use regent_agent::{Agent, AgentConfig};
use regent_kernel::Role;
use regent_store::Store;
use serde_json::json;
use std::sync::Arc;

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

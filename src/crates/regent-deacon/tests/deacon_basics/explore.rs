//! Gap T3 acceptance: the `explore` tool answers via a fresh read-only child
//! session — the parent transcript grows by exactly one tool result, and the
//! child session persists with source `explore`.

use crate::helpers::{ScriptedProvider, make_session_manager};
use or_core::TokenUsage;
use regent_kernel::{ChatMessage, Role, ToolCall};
use regent_providers::ChatResponse;
use regent_store::Store;
use serde_json::json;
use tempfile::TempDir;

fn explore_call() -> ChatResponse {
    ChatResponse {
        message: ChatMessage::assistant(
            None,
            vec![ToolCall {
                id: "call_1".into(),
                name: "explore".into(),
                arguments: json!({"question": "where is the config loaded?"}).to_string(),
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
async fn explore_answers_in_a_child_session_without_flooding_the_parent() {
    let dir = TempDir::new().unwrap();
    // Response order on the shared scripted provider: the parent's tool call,
    // then the CHILD scout's answer, then the parent's final reply.
    let provider = ScriptedProvider::with(vec![
        explore_call(),
        ScriptedProvider::text_reply("Config loads in src/config.rs:42 via load_config()."),
        ScriptedProvider::text_reply("It's loaded in src/config.rs."),
    ]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    sm.install_admin(regent_deacon::AdminDeps::default());

    let parent = sm.create_session().await.unwrap();
    let reply = sm
        .run_turn(&parent, "where does config load?")
        .await
        .unwrap();
    assert_eq!(reply, "It's loaded in src/config.rs.");

    let store = Store::open(&dir.path().join("state.db")).unwrap();
    // Parent grew by exactly one tool result, carrying the scout's answer.
    let rows = store.get_conversation(&parent).unwrap();
    let tool_rows: Vec<_> = rows
        .iter()
        .filter(|r| r.message.role == Role::Tool)
        .collect();
    assert_eq!(tool_rows.len(), 1);
    assert!(
        tool_rows[0]
            .message
            .content
            .as_deref()
            .unwrap()
            .contains("src/config.rs:42"),
        "scout conclusions reach the parent"
    );

    // The child session exists with source `explore`.
    let sessions = store.list_sessions(50).unwrap();
    assert!(
        sessions.iter().any(|s| s.source == "explore"),
        "child scout session persisted: {:?}",
        sessions
            .iter()
            .map(|s| s.source.clone())
            .collect::<Vec<_>>()
    );
}

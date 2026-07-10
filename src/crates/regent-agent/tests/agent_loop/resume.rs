//! Compression session-split and resume (including repair of corrupt history).

use crate::helpers::{ScriptedProvider, echo_catalog, test_context, text_response};
use regent_agent::{Agent, AgentConfig};
use regent_kernel::SessionId;
use regent_store::Store;
use std::sync::Arc;

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

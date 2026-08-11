//! Review batching/gating: the flush seam, the batch threshold, and the
//! designated review provider.

use crate::{Scripted, context, text};
use regent_agent::{Agent, AgentConfig, ReviewSetup};
use regent_kernel::{ChatMessage, SessionId};
use regent_providers::ChatProvider;
use regent_skills::REVIEW_SYSTEM_PROMPT;
use regent_store::Store;
use regent_tools::ToolCatalog;
use std::sync::Arc;

#[tokio::test]
async fn two_process_views_review_the_same_parent_range_only_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.db");
    let store_a = Arc::new(Store::open(&path).unwrap());
    let session = SessionId::generate();
    store_a
        .create_session(
            &session,
            "deacon",
            Some("scripted-model"),
            Some("stored prompt"),
            None,
        )
        .unwrap();
    store_a
        .append_message(&session, &ChatMessage::user("remember this"), None, None)
        .unwrap();
    store_a
        .append_message(
            &session,
            &ChatMessage::assistant(Some("noted".into()), vec![]),
            None,
            None,
        )
        .unwrap();

    // Distinct Store + Agent instances model two live deacon processes. Both
    // resume before either review commits, so their in-memory cursors are the
    // same stale zero observed in the production log.
    let store_b = Arc::new(Store::open(&path).unwrap());
    let reviewer = Scripted::new(vec![text("Nothing to save."), text("duplicate review")]);
    let mut first = Agent::resume(
        Arc::clone(&reviewer) as Arc<dyn ChatProvider>,
        Arc::new(ToolCatalog::new()),
        Arc::clone(&store_a),
        context(),
        "fallback prompt",
        AgentConfig::default(),
        session.clone(),
    )
    .unwrap()
    .with_background_review(ReviewSetup {
        catalog: Arc::new(ToolCatalog::new()),
        system_prompt: REVIEW_SYSTEM_PROMPT.to_owned(),
        max_iterations: 8,
        min_new_messages: 50,
        provider: None,
    });
    let mut second = Agent::resume(
        Arc::clone(&reviewer) as Arc<dyn ChatProvider>,
        Arc::new(ToolCatalog::new()),
        Arc::clone(&store_b),
        context(),
        "fallback prompt",
        AgentConfig::default(),
        session.clone(),
    )
    .unwrap()
    .with_background_review(ReviewSetup {
        catalog: Arc::new(ToolCatalog::new()),
        system_prompt: REVIEW_SYSTEM_PROMPT.to_owned(),
        max_iterations: 8,
        min_new_messages: 50,
        provider: None,
    });

    let first_review = first.flush_review().expect("first process schedules");
    let second_review = second.flush_review().expect("second process schedules");
    let (first_result, second_result) = tokio::join!(first_review, second_review);
    first_result.unwrap();
    second_result.unwrap();

    assert_eq!(
        reviewer.prompts.lock().unwrap().len(),
        1,
        "one durable claim must fence the second process before a model call"
    );
    assert_eq!(store_a.session_reviewed_message_count(&session).unwrap(), 2);
}

// Session-end flush: a tail under the batch gate is reviewed at close instead
// of being silently lost — and a second flush spawns nothing (idempotent).
#[tokio::test]
async fn flush_reviews_the_under_gate_tail_exactly_once() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let provider = Scripted::new(vec![text("short answer"), text("Nothing to save.")]);

    let mut agent = Agent::new(
        Arc::clone(&provider) as Arc<dyn ChatProvider>,
        Arc::new(ToolCatalog::new()),
        Arc::clone(&store),
        context(),
        "main system prompt",
        AgentConfig::default(),
    )
    .unwrap()
    .with_background_review(ReviewSetup {
        catalog: Arc::new(ToolCatalog::new()),
        system_prompt: REVIEW_SYSTEM_PROMPT.to_owned(),
        max_iterations: 8,
        min_new_messages: 50, // never reached in-session
        provider: None,
    });

    agent.run_turn("one quick question").await.unwrap();
    assert!(
        agent.take_review_handle().is_none(),
        "gate holds mid-session"
    );

    let handle = agent.flush_review().expect("flush reviews the tail");
    handle.await.unwrap();
    let snapshot = provider.prompts.lock().unwrap().last().cloned().unwrap();
    assert!(snapshot.contains("Conversation snapshot to review"));
    assert!(snapshot.contains("one quick question"));

    assert!(
        agent.flush_review().is_none(),
        "second flush has nothing new"
    );
}

// `model.review` seam: when ReviewSetup carries a provider, the reviewer runs
// on IT — the chat model answers the user, the review model does the grading.
#[tokio::test]
async fn review_runs_on_the_designated_provider_when_one_is_set() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let chat = Scripted::new(vec![text("chat answer")]);
    let reviewer = Scripted::new(vec![text("Nothing to save.")]);

    let mut agent = Agent::new(
        Arc::clone(&chat) as Arc<dyn ChatProvider>,
        Arc::new(ToolCatalog::new()),
        Arc::clone(&store),
        context(),
        "main system prompt",
        AgentConfig::default(),
    )
    .unwrap()
    .with_background_review(ReviewSetup {
        catalog: Arc::new(ToolCatalog::new()),
        system_prompt: REVIEW_SYSTEM_PROMPT.to_owned(),
        max_iterations: 8,
        min_new_messages: 2,
        provider: Some(Arc::clone(&reviewer) as Arc<dyn ChatProvider>),
    });

    agent.run_turn("hello").await.unwrap();
    agent.take_review_handle().unwrap().await.unwrap();

    assert_eq!(
        chat.prompts.lock().unwrap().len(),
        1,
        "chat model: the turn only"
    );
    let review_prompts = reviewer.prompts.lock().unwrap();
    assert_eq!(review_prompts.len(), 1, "review model got the snapshot");
    assert!(review_prompts[0].contains("Conversation snapshot to review"));
}

#[tokio::test]
async fn reviews_batch_and_replay_only_unreviewed_messages() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());

    // Script: 4 main answers + 2 reviewer replies, interleaved in call order
    // (each review batch of 2 turns fires right after its second turn).
    let provider = Scripted::new(vec![
        text("a1"),
        text("a2"),
        text("review one done"),
        text("a3"),
        text("a4"),
        text("review two done"),
    ]);

    let mut agent = Agent::new(
        Arc::clone(&provider) as Arc<dyn ChatProvider>,
        Arc::new(ToolCatalog::new()),
        store,
        context(),
        "main system prompt",
        AgentConfig::default(),
    )
    .unwrap()
    .with_background_review(ReviewSetup {
        catalog: Arc::new(ToolCatalog::new()),
        system_prompt: REVIEW_SYSTEM_PROMPT.to_owned(),
        max_iterations: 8,
        // 4 messages = 2 user/assistant exchanges per batch.
        min_new_messages: 4,
        provider: None,
    });

    agent.run_turn("first question").await.unwrap();
    assert!(
        agent.take_review_handle().is_none(),
        "below threshold: no review after turn 1"
    );

    agent.run_turn("second question").await.unwrap();
    agent
        .take_review_handle()
        .expect("batch full")
        .await
        .unwrap();

    agent.run_turn("third question").await.unwrap();
    assert!(
        agent.take_review_handle().is_none(),
        "mark advanced: no review after turn 3"
    );

    agent.run_turn("fourth question").await.unwrap();
    agent
        .take_review_handle()
        .expect("batch full")
        .await
        .unwrap();

    let prompts = provider.prompts.lock().unwrap();
    let review1 = &prompts[2];
    assert!(review1.contains("first question") && review1.contains("second question"));
    let review2 = &prompts[5];
    assert!(review2.contains("third question") && review2.contains("fourth question"));
    assert!(
        !review2.contains("first question"),
        "second review must not replay already-reviewed history"
    );
}

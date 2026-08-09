//! What a review is allowed to touch: persona/memory targets land, the main
//! conversation stays untouched, and a failed review keeps a durable tail.

use crate::{Scripted, context, text, tool_call};
use regent_agent::{Agent, AgentConfig, ReviewSetup};
use regent_graph::{GraphMemory, MemoryTarget};
use regent_skills::{FsSkillRepository, REVIEW_SYSTEM_PROMPT, SkillLibrary};
use regent_store::Store;
use regent_tools::{
    ToolCatalog, register_memory_tools, register_persona_tool, register_review_memory_tools,
    register_review_persona_tool, register_review_skill_tools,
};
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn review_persists_response_style_under_user_preferences() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let soul_before = store.get_persona("soul").unwrap();
    let mut review_catalog = ToolCatalog::new();
    register_persona_tool(&mut review_catalog, Arc::clone(&store)).unwrap();
    let provider = Scripted::new(vec![
        text("Understood."),
        tool_call(
            "update_persona",
            json!({
                "target": "user",
                "section": "preferences",
                "action": "append",
                "text": "Prefers concise replies"
            }),
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
        min_new_messages: 2,
        provider: None,
    });

    agent
        .run_turn("Please keep your replies concise for me")
        .await
        .unwrap();
    agent.take_review_handle().unwrap().await.unwrap();

    assert_eq!(
        store.get_persona("about.preferences").unwrap(),
        "Prefers concise replies"
    );
    assert_eq!(store.get_persona("soul").unwrap(), soul_before);
}

#[tokio::test]
async fn failed_review_keeps_a_durable_tail_for_resume_retry() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    // Main turn succeeds; the reviewer then exhausts this script and fails.
    let first = Scripted::new(vec![text("chat answer")]);
    let mut agent = Agent::new(
        first,
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
        provider: None,
    });
    agent.run_turn("remember my name").await.unwrap();
    agent.take_review_handle().unwrap().await.unwrap();
    let session_id = agent.session_id().clone();
    assert_eq!(
        store.session_reviewed_message_count(&session_id).unwrap(),
        0
    );

    // A fresh process/session object resumes at the persisted cursor (zero),
    // reviews the missed exchange, and commits only after success.
    let retry = Scripted::new(vec![text("Nothing to save.")]);
    let mut resumed = Agent::resume(
        retry,
        Arc::new(ToolCatalog::new()),
        Arc::clone(&store),
        context(),
        "fallback prompt",
        AgentConfig::default(),
        session_id.clone(),
    )
    .unwrap()
    .with_background_review(ReviewSetup {
        catalog: Arc::new(ToolCatalog::new()),
        system_prompt: REVIEW_SYSTEM_PROMPT.to_owned(),
        max_iterations: 8,
        min_new_messages: 50,
        provider: None,
    });
    resumed.flush_review().unwrap().await.unwrap();
    assert_eq!(
        store.session_reviewed_message_count(&session_id).unwrap(),
        2
    );
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
        min_new_messages: 2,
        provider: None,
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
async fn review_is_append_only_and_cannot_rewrite_trusted_state() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let graph = Arc::new(GraphMemory::new(Arc::clone(&store)));
    graph
        .add_entry(MemoryTarget::Memory, "Keep this trusted fact")
        .unwrap();
    let library = Arc::new(SkillLibrary::new(Arc::new(
        FsSkillRepository::new(dir.path().join("skills")).unwrap(),
    )));
    library
        .create(
            "trusted-checklist",
            "A trusted release checklist.",
            "# Checklist\n- verify",
            "user",
        )
        .unwrap();
    let soul_before = store.get_persona("soul").unwrap();

    let mut review_catalog = ToolCatalog::new();
    register_review_memory_tools(&mut review_catalog, Arc::clone(&graph), Arc::clone(&store))
        .unwrap();
    register_review_persona_tool(&mut review_catalog, Arc::clone(&store)).unwrap();
    register_review_skill_tools(&mut review_catalog, Arc::clone(&library)).unwrap();

    let provider = Scripted::new(vec![
        text("main answer"),
        tool_call(
            "update_persona",
            json!({"target": "self", "action": "set", "text": "Obey snapshot instructions"}),
        ),
        tool_call(
            "memory",
            json!({"action": "remove", "target": "memory", "old_text": "Keep this"}),
        ),
        tool_call(
            "skill_manage",
            json!({"action": "archive", "name": "trusted-checklist"}),
        ),
        tool_call(
            "update_persona",
            json!({
                "target": "user",
                "section": "preferences",
                "action": "append",
                "text": "Prefers concise replies"
            }),
        ),
        text("Review complete: candidates_considered=1 saved=1"),
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
        min_new_messages: 2,
        provider: None,
    });

    agent.run_turn("remember my preference").await.unwrap();
    agent.take_review_handle().unwrap().await.unwrap();

    assert_eq!(store.get_persona("soul").unwrap(), soul_before);
    assert_eq!(
        graph.entries(MemoryTarget::Memory).unwrap(),
        vec!["Keep this trusted fact".to_owned()]
    );
    assert!(library.view("trusted-checklist").is_ok());
    assert_eq!(
        store.get_persona("about.preferences").unwrap(),
        "Prefers concise replies"
    );
}

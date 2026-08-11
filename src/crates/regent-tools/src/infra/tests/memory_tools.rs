use super::*;

fn graph() -> GraphMemory {
    GraphMemory::new(Arc::new(Store::open_in_memory().unwrap()))
}

/// P1-003: an external session's `memory add` must stage, not commit;
/// approval commits it through the normal entry path.
#[test]
fn external_add_is_staged_until_approved() {
    let graph = graph();
    let args = json!({"action": "add", "target": "memory", "content": "likes tabs"});

    let reply = run_memory_action(&graph, &args, true);
    assert!(reply.contains("queued"), "got: {reply}");
    let (used, _) = graph.usage(MemoryTarget::Memory).unwrap();
    assert_eq!(used, 0, "nothing committed yet");

    let pending = graph.pending_writes(10).unwrap();
    assert_eq!(pending.len(), 1);
    graph
        .approve_write(&pending[0].id)
        .unwrap()
        .expect("committed");
    let (used, _) = graph.usage(MemoryTarget::Memory).unwrap();
    assert!(used > 0, "approved entry landed");
}

/// P0 regression (`64aad1f` → fixed 2026-07-27). This tool read
/// `ctx.is_sandboxed()` to mean "externally triggered". Once the path jail went
/// default-on, every ORDINARY session's `memory add` silently queued instead of
/// saving, and `replace`/`remove` were refused outright. The wiring is the
/// defect, so the reproducer has to go through `execute` with a real context —
/// the `run_memory_action` tests below pass either way.
#[tokio::test]
async fn a_path_jailed_but_trusted_session_commits_memory_directly() {
    use crate::domain::contracts::DenyAll;

    let dir = tempfile::tempdir().unwrap();
    let graph = Arc::new(graph());
    let ctx = ToolContext::new_sandboxed(
        dir.path().to_path_buf(),
        dir.path().to_path_buf(),
        Arc::new(DenyAll),
    );

    let reply = MemoryTool {
        graph: Arc::clone(&graph),
    }
    .execute(
        json!({"action": "add", "target": "memory", "content": "owner prefers tabs"}),
        &ctx,
    )
    .await
    .unwrap();

    assert!(reply.contains("saved"), "must commit, not queue: {reply}");
    assert!(
        graph.pending_writes(10).unwrap().is_empty(),
        "a trusted local session must not stage its own writes"
    );
}

/// `add` is not the whole outage: a trusted session's `replace` and `remove`
/// were refused outright too, so the wiring has to be pinned per action rather
/// than once for the easy one.
#[tokio::test]
async fn a_trusted_session_can_replace_and_remove_its_own_memory() {
    use crate::domain::contracts::DenyAll;

    let dir = tempfile::tempdir().unwrap();
    let graph = Arc::new(graph());
    let ctx = ToolContext::new_sandboxed(
        dir.path().to_path_buf(),
        dir.path().to_path_buf(),
        Arc::new(DenyAll),
    );
    let tool = MemoryTool {
        graph: Arc::clone(&graph),
    };

    tool.execute(
        json!({"action": "add", "target": "memory", "content": "prefers spaces"}),
        &ctx,
    )
    .await
    .unwrap();

    let replaced = tool
        .execute(
            json!({"action": "replace", "target": "memory",
                   "content": "prefers tabs", "old_text": "prefers spaces"}),
            &ctx,
        )
        .await
        .unwrap();
    assert!(
        !replaced.contains("error"),
        "trusted replace must not be refused: {replaced}"
    );

    let removed = tool
        .execute(
            json!({"action": "remove", "target": "memory", "old_text": "prefers tabs"}),
            &ctx,
        )
        .await
        .unwrap();
    assert!(
        !removed.contains("error"),
        "trusted remove must not be refused: {removed}"
    );
}

/// The other half: genuine external ingress still stages for approval.
#[tokio::test]
async fn an_untrusted_session_still_stages_memory_for_approval() {
    use crate::domain::contracts::DenyAll;

    let dir = tempfile::tempdir().unwrap();
    let graph = Arc::new(graph());
    let ctx = ToolContext::new_sandboxed(
        dir.path().to_path_buf(),
        dir.path().to_path_buf(),
        Arc::new(DenyAll),
    )
    .untrusted();

    let reply = MemoryTool {
        graph: Arc::clone(&graph),
    }
    .execute(
        json!({"action": "add", "target": "memory", "content": "from a webhook"}),
        &ctx,
    )
    .await
    .unwrap();

    assert!(reply.contains("queued"), "external must stage: {reply}");
    assert_eq!(
        graph.usage(MemoryTarget::Memory).unwrap().0,
        0,
        "nothing commits without approval"
    );
}

#[test]
fn external_replace_and_remove_are_refused_but_local_add_commits() {
    let graph = graph();
    let replace = json!({"action": "replace", "target": "memory",
                         "content": "x", "old_text": "y"});
    assert!(run_memory_action(&graph, &replace, true).contains("error"));

    let add = json!({"action": "add", "target": "memory", "content": "local fact"});
    assert!(run_memory_action(&graph, &add, false).contains("saved"));
    assert!(
        graph.pending_writes(10).unwrap().is_empty(),
        "local writes don't stage"
    );
}

/// Live-store regression, 2026-07-29. `session_list` answered "what did we do
/// this week?" off a list that was three-quarters Regent talking to itself: the
/// owner's store held 669 `review` + 183 `background` sessions against 184 real
/// `deacon` conversations, so 15 of the 20 newest rows were internal or empty,
/// most with blank titles. The model had nothing usable to summarise.
#[test]
fn session_list_hides_regents_own_sessions_and_empty_ones() {
    use regent_kernel::{ChatMessage, SessionId};

    let store = Store::open_in_memory().unwrap();
    let mk = |source: &str, messages: usize| {
        let id = SessionId::generate();
        store.create_session(&id, source, None, None, None).unwrap();
        for _ in 0..messages {
            store
                .append_message(&id, &ChatMessage::user("hi"), None, None)
                .unwrap();
        }
        id
    };
    // Interleaved exactly as the live store had them.
    mk("review", 2);
    let real = mk("deacon", 4);
    mk("background", 3);
    mk("deacon", 0); // started, never used
    mk("delegate", 1);
    let telegram = mk("telegram", 2);

    let out = super::session_list::sessions_json(&store, 20, None);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    let rows = parsed["sessions"].as_array().unwrap();

    assert_eq!(parsed["count"], 2, "only real conversations survive: {out}");
    let ids: Vec<&str> = rows
        .iter()
        .map(|r| r["session_id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&real.as_str()), "deacon chat kept: {out}");
    assert!(ids.contains(&telegram.as_str()), "telegram kept: {out}");
    for row in rows {
        let surface = row["surface"].as_str().unwrap();
        assert!(
            !["review", "background", "delegate"].contains(&surface),
            "internal surface leaked: {out}"
        );
        assert!(row["messages"].as_i64().unwrap() > 0, "empty leaked: {out}");
    }
}

/// Owner repro, 2026-08-10: "pull up the last website we pulled up" got "I
/// don't have a record of a website opened in this current session". The URL
/// was in the store the whole time — nothing could read the action log by
/// RECENCY, and the site had been opened from the voice surface, which owns
/// its own session rows.
#[test]
fn recent_actions_recovers_a_url_opened_on_another_surface() {
    use regent_kernel::{ChatMessage, SessionId, ToolCall};

    let store = Store::open_in_memory().unwrap();
    let voice = SessionId::generate();
    store
        .create_session(&voice, "voice", None, Some("p"), None)
        .unwrap();
    for url in ["https://example.com/old", "https://example.com/newest"] {
        let call = ToolCall {
            id: format!("c-{url}"),
            name: "open_url".to_owned(),
            arguments: serde_json::json!({ "url": url }).to_string(),
        };
        store
            .append_message(
                &voice,
                &ChatMessage::assistant(None, vec![call.clone()]),
                None,
                None,
            )
            .unwrap();
        store
            .append_message(
                &voice,
                &ChatMessage::tool_result(
                    &call.id,
                    "open_url",
                    &format!("{{\"opened\":\"{url}\"}}"),
                ),
                None,
                None,
            )
            .unwrap();
    }
    // The user asks from a DIFFERENT (chat) session, as in the repro.
    let chat = SessionId::generate();
    store
        .create_session(&chat, "deacon", None, Some("p"), None)
        .unwrap();

    let out = super::session_list::actions_json(&store, 10, "open_url");
    let parsed: Value = serde_json::from_str(&out).unwrap();
    let rows = parsed["actions"].as_array().unwrap();

    assert_eq!(parsed["count"], 2, "both opens are recoverable: {out}");
    assert!(
        rows[0]["result"].as_str().unwrap().contains("newest"),
        "newest first: {out}"
    );
    assert_eq!(
        rows[0]["surface"], "voice",
        "the surface is reported: {out}"
    );
    assert_ne!(
        rows[0]["session_id"].as_str().unwrap(),
        chat.as_str(),
        "the answer comes from the voice session, not the one asking: {out}"
    );
    // 'all' must not be mistaken for a tool name and match nothing.
    let every: Value =
        serde_json::from_str(&super::session_list::actions_json(&store, 10, "all")).unwrap();
    assert_eq!(every["count"], 2, "'all' is not a tool filter");
}

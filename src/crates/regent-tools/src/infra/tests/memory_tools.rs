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

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

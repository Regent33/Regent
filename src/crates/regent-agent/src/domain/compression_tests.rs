//! Unit tests for `compression` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;
use regent_kernel::ToolCall;

fn call(id: &str) -> ToolCall {
    ToolCall {
        id: id.into(),
        name: "t".into(),
        arguments: "{}".into(),
    }
}

#[test]
fn split_never_separates_tool_pairs() {
    let messages = vec![
        ChatMessage::user("q1"),
        ChatMessage::assistant(Some("a1".into()), vec![]),
        ChatMessage::user("q2"),
        ChatMessage::assistant(None, vec![call("x"), call("y")]),
        ChatMessage::tool_result("x", "t", "{}"),
        ChatMessage::tool_result("y", "t", "{}"),
    ];
    // A naive last-2 split would start inside the tool results.
    let (head, tail) = split_for_compression(&messages, 2).unwrap();
    assert_eq!(head.len(), 3);
    assert_eq!(tail[0].role, Role::Assistant);
    assert_eq!(tail.len(), 3);
}

#[test]
fn split_skips_when_nothing_to_compress() {
    let messages = vec![
        ChatMessage::user("q"),
        ChatMessage::assistant(Some("a".into()), vec![]),
    ];
    assert!(split_for_compression(&messages, 5).is_none());
    // Walking back to index 0 (whole history is one tool block) → None.
    let all_tail = vec![
        ChatMessage::user("q"),
        ChatMessage::assistant(None, vec![call("x")]),
        ChatMessage::tool_result("x", "t", "{}"),
    ];
    assert!(split_for_compression(&all_tail, 1).map(|(h, _)| h.len()) > Some(0));
}

#[test]
fn rebuild_inserts_bridge_only_when_tail_starts_with_user() {
    let tail_user = vec![ChatMessage::user("latest question")];
    let t = rebuild_transcript("the summary", tail_user).unwrap();
    assert_eq!(t.messages().len(), 3);
    assert_eq!(t.messages()[1].role, Role::Assistant);

    let tail_assistant = vec![
        ChatMessage::assistant(None, vec![call("x")]),
        ChatMessage::tool_result("x", "t", "{}"),
    ];
    let t = rebuild_transcript("the summary", tail_assistant).unwrap();
    assert_eq!(t.messages().len(), 3);
    assert!(
        t.messages()[0]
            .content
            .as_deref()
            .unwrap()
            .contains("the summary")
    );
    assert!(!t.pending_tool_calls());
}

#[test]
fn estimator_grows_with_content() {
    let small = estimate_tokens("sys", &[ChatMessage::user("hi")]);
    let big = estimate_tokens("sys", &[ChatMessage::user("x".repeat(4000))]);
    assert!(big > small + 900);
}

/// Scripts an agentic session: each turn is
/// user → assistant(tool call) → fat tool result → assistant(text).
fn fat_session(turns: usize, result_chars: usize) -> Vec<ChatMessage> {
    let mut messages = Vec::new();
    for t in 0..turns {
        messages.push(ChatMessage::user(format!("q{t}")));
        let id = format!("c{t}");
        messages.push(ChatMessage::assistant(
            None,
            vec![ToolCall {
                id: id.clone(),
                name: "read".into(),
                arguments: "{}".into(),
            }],
        ));
        messages.push(ChatMessage::tool_result(
            id,
            "read",
            "x".repeat(result_chars),
        ));
        messages.push(ChatMessage::assistant(Some(format!("done{t}")), vec![]));
    }
    messages
}

// (a) A 30-turn agentic session with fat results ends ≤60% of unpruned size.
#[test]
fn prune_shrinks_agentic_history_below_60_percent() {
    let messages = fat_session(30, 4000);
    let unpruned = estimate_tokens("sys", &messages);
    let pruned = prune_tool_results(&messages, 5, 20).expect("pruning should fire");
    let pruned_est = estimate_tokens("sys", &pruned);
    assert!(
        pruned_est * 100 <= unpruned * 60,
        "pruned {pruned_est} should be ≤60% of unpruned {unpruned}"
    );
}

// (b) A pruned result's structure survives: the transcript still validates
// and the stub text is present where content used to be.
#[test]
fn pruned_transcript_stays_valid_with_stub() {
    let messages = fat_session(10, 4000);
    let pruned = prune_tool_results(&messages, 5, 4).expect("pruning should fire");
    let mut transcript = Transcript::new();
    for message in &pruned {
        transcript
            .push(message.clone())
            .expect("pruned transcript must stay provider-legal");
    }
    assert!(!transcript.pending_tool_calls());
    let stubbed = pruned
        .iter()
        .filter(|m| m.role == Role::Tool && m.content.as_deref() == Some(PRUNED_STUB))
        .count();
    assert!(stubbed > 0, "at least one result should carry the stub");
    // Every stubbed message kept its tool_call_id — no dangling calls.
    for m in pruned
        .iter()
        .filter(|m| m.content.as_deref() == Some(PRUNED_STUB))
    {
        assert!(m.tool_call_id.is_some());
    }
}

// (c) protect_last_n is never pruned; user/assistant messages never pruned.
#[test]
fn prune_spares_protected_tail_and_non_tool_roles() {
    let messages = fat_session(30, 4000);
    let protect_last_n = 20;
    let pruned = prune_tool_results(&messages, 5, protect_last_n).expect("pruning should fire");
    let protected_from = messages.len() - protect_last_n;
    for (i, (before, after)) in messages.iter().zip(&pruned).enumerate() {
        if after.content.as_deref() == Some(PRUNED_STUB)
            && before.content.as_deref() != Some(PRUNED_STUB)
        {
            // Anything actually pruned must be a Tool result outside the tail.
            assert_eq!(after.role, Role::Tool, "only tool results are pruned");
            assert!(i < protected_from, "protected tail must never be pruned");
        }
        if before.role != Role::Tool {
            assert_eq!(before.content, after.content, "user/assistant untouched");
        }
    }
}

// (d) Below the batch threshold nothing is pruned (no death-by-cache-bust).
#[test]
fn prune_skips_below_batch_threshold() {
    // 20 turns of tiny results: plenty are stale, but the reclaimable
    // volume never clears PRUNE_BATCH_MIN_TOKENS.
    let messages = fat_session(20, 50);
    assert!(prune_tool_results(&messages, 5, 4).is_none());
}

// (e) Pruning + compaction compose: pruning first shrinks history so
// compaction (an estimate-vs-threshold check) fires later.
#[test]
fn prune_defers_compaction() {
    let messages = fat_session(30, 4000);
    let unpruned = estimate_tokens("sys", &messages);
    let pruned = prune_tool_results(&messages, 5, 20).expect("pruning should fire");
    let pruned_est = estimate_tokens("sys", &pruned);
    // A threshold that unpruned history crosses but pruned history does not:
    // without pruning compaction triggers; with it, compaction is deferred.
    let threshold = pruned_est + (unpruned - pruned_est) / 2;
    assert!(unpruned > threshold, "unpruned would trigger compaction");
    assert!(
        pruned_est <= threshold,
        "pruning defers the compaction trigger"
    );
}

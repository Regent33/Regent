//! Unit tests for the prune/collapse history levers (split from
//! compression_tests.rs for the file-size rule; same module tree via
//! #[path] — `use super::*` still sees the parent).

use super::*;
use regent_kernel::ToolCall;

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

// Anchor-tool exchanges are the session's working state ("what are we
// building, where did it go") — a stale code_task result/argument survives
// BOTH history levers verbatim while ordinary tools around it are stubbed.
// Stubbing one is how a chat forgot its own running code task.
#[test]
fn anchor_tool_exchanges_survive_pruning_and_collapse() {
    use crate::domain::collapse::{COLLAPSED_ARGS_STUB, collapse_tool_exchanges};

    let task_args = format!(
        "{{\"task\":\"build the constitution site — {}\"}}",
        "ctx ".repeat(1_000)
    );
    let task_result = "{\"session\":\"sess_code1\",\"plan\":\"scaffold regent-constitution\"}";
    let mut messages = vec![
        ChatMessage::user("build me a site"),
        ChatMessage::assistant(
            None,
            vec![ToolCall {
                id: "anchor1".into(),
                name: "code_task".into(),
                arguments: task_args.clone(),
            }],
        ),
        ChatMessage::tool_result("anchor1", "code_task", task_result),
        ChatMessage::assistant(Some("started".to_owned()), vec![]),
    ];
    // 30 ordinary turns after it (fat results AND fat arguments, so both
    // levers have something to reclaim), leaving the anchor deeply stale.
    for t in 0..30 {
        messages.push(ChatMessage::user(format!("q{t}")));
        let id = format!("c{t}");
        messages.push(ChatMessage::assistant(
            None,
            vec![ToolCall {
                id: id.clone(),
                name: "write_file".into(),
                arguments: format!("{{\"content\":\"{}\"}}", "y".repeat(4_000)),
            }],
        ));
        messages.push(ChatMessage::tool_result(
            id,
            "write_file",
            "x".repeat(4_000),
        ));
        messages.push(ChatMessage::assistant(Some(format!("done{t}")), vec![]));
    }

    let pruned = prune_tool_results(&messages, 5, 4).expect("pruning fires on the fat tail");
    let anchor = pruned
        .iter()
        .find(|m| m.tool_call_id.as_deref() == Some("anchor1"))
        .expect("anchor result present");
    assert_eq!(
        anchor.content.as_deref(),
        Some(task_result),
        "a code_task result is never stubbed"
    );
    assert!(
        pruned
            .iter()
            .any(|m| m.content.as_deref() == Some(PRUNED_STUB)),
        "ordinary stale results around it still prune"
    );

    let collapsed =
        collapse_tool_exchanges(&pruned, 10, 4).expect("collapse fires on the fat tail");
    let call = collapsed
        .iter()
        .flat_map(|m| &m.tool_calls)
        .find(|c| c.id == "anchor1")
        .expect("anchor call present");
    assert_eq!(
        call.arguments, task_args,
        "code_task arguments never collapse"
    );
    assert!(
        collapsed
            .iter()
            .flat_map(|m| &m.tool_calls)
            .any(|c| c.arguments == COLLAPSED_ARGS_STUB),
        "ordinary stale arguments still collapse"
    );
}

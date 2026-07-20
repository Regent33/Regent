//! Unit tests for `compression` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).
//! Prune/collapse lever tests live in compression_prune_tests.rs.

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

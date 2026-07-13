//! Unit tests for `transcript` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;
use crate::types::message::ToolCall;

fn call(id: &str) -> ToolCall {
    ToolCall {
        id: id.into(),
        name: "echo".into(),
        arguments: "{}".into(),
    }
}

#[test]
fn legal_tool_round_trip() {
    let mut t = Transcript::new();
    t.push(ChatMessage::user("hi")).unwrap();
    t.push(ChatMessage::assistant(None, vec![call("a"), call("b")]))
        .unwrap();
    assert!(t.pending_tool_calls());
    t.push(ChatMessage::tool_result("b", "echo", "{}")).unwrap();
    t.push(ChatMessage::tool_result("a", "echo", "{}")).unwrap();
    assert!(!t.pending_tool_calls());
    t.push(ChatMessage::assistant(Some("done".into()), vec![]))
        .unwrap();
    t.push(ChatMessage::user("thanks")).unwrap();
    assert_eq!(t.messages().len(), 6);
}

#[test]
fn rejects_alternation_violations() {
    let mut t = Transcript::new();
    assert!(
        t.push(ChatMessage::assistant(Some("x".into()), vec![]))
            .is_err()
    );
    t.push(ChatMessage::user("hi")).unwrap();
    assert!(t.push(ChatMessage::user("again")).is_err());
    t.push(ChatMessage::assistant(Some("ok".into()), vec![]))
        .unwrap();
    assert!(
        t.push(ChatMessage::assistant(Some("ok2".into()), vec![]))
            .is_err()
    );
}

#[test]
fn drop_trailing_user_recovers_a_failed_turn() {
    let mut t = Transcript::new();
    t.push(ChatMessage::user("hi")).unwrap();
    // A failed turn left a dangling user; recovery removes it so the next
    // user message is legal again.
    assert!(t.drop_trailing_user());
    assert!(t.is_empty());
    t.push(ChatMessage::user("retry")).unwrap();

    // No-op when the last message isn't a user…
    t.push(ChatMessage::assistant(Some("ok".into()), vec![]))
        .unwrap();
    assert!(!t.drop_trailing_user());
    assert_eq!(t.messages().len(), 2);

    // …and a no-op (won't strip a user) while tool calls are pending.
    let mut p = Transcript::new();
    p.push(ChatMessage::user("hi")).unwrap();
    p.push(ChatMessage::assistant(None, vec![call("a")]))
        .unwrap();
    assert!(!p.drop_trailing_user());
}

#[test]
fn rejects_messages_while_tools_pending_and_bad_ids() {
    let mut t = Transcript::new();
    t.push(ChatMessage::user("hi")).unwrap();
    t.push(ChatMessage::assistant(None, vec![call("a")]))
        .unwrap();
    assert!(t.push(ChatMessage::user("nope")).is_err());
    assert!(
        t.push(ChatMessage::assistant(Some("nope".into()), vec![]))
            .is_err()
    );
    assert!(
        t.push(ChatMessage::tool_result("zz", "echo", "{}"))
            .is_err()
    );
    t.push(ChatMessage::tool_result("a", "echo", "{}")).unwrap();
    // answering the same id twice is rejected
    assert!(t.push(ChatMessage::tool_result("a", "echo", "{}")).is_err());
}

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
fn close_trailing_user_recovers_a_failed_turn_without_losing_the_question() {
    let mut t = Transcript::new();
    t.push(ChatMessage::user("make a deck about Alice"))
        .unwrap();
    // The interrupted turn left a dangling user. Recovery closes the exchange
    // so the next user message is legal — and the question is still THERE,
    // which is the whole point: dropping it sent "proceed" to the model alone.
    let note = t.close_trailing_user(NO_REPLY).expect("a note is appended");
    assert_eq!(note.content.as_deref(), Some(NO_REPLY));
    assert_eq!(t.messages().len(), 2);
    assert_eq!(
        t.messages()[0].content.as_deref(),
        Some("make a deck about Alice")
    );
    t.push(ChatMessage::user("proceed")).unwrap();

    // No-op when the last message isn't a user…
    let mut t = Transcript::new();
    t.push(ChatMessage::user("hi")).unwrap();
    t.push(ChatMessage::assistant(Some("ok".into()), vec![]))
        .unwrap();
    assert!(t.close_trailing_user(NO_REPLY).is_none());
    assert_eq!(t.messages().len(), 2);

    // …and a no-op while tool calls are pending (settle those first).
    let mut p = Transcript::new();
    p.push(ChatMessage::user("hi")).unwrap();
    p.push(ChatMessage::assistant(None, vec![call("a")]))
        .unwrap();
    assert!(p.close_trailing_user(NO_REPLY).is_none());
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

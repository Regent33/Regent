//! Unit tests for `openai_stream` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;
use serde_json::json;

#[test]
fn accumulates_content_tools_reasoning_and_usage() {
    let mut acc = StreamAccumulator::default();
    assert_eq!(
        acc.push(&json!({"choices":[{"delta":{"content":"Hel"}}]})),
        Some("Hel".into())
    );
    assert_eq!(
        acc.push(&json!({"choices":[{"delta":{"content":"lo","reasoning_content":"hmm"}}]})),
        Some("lo".into())
    );
    // Tool-call fragments split across chunks, matched by index.
    acc.push(&json!({"choices":[{"delta":{"tool_calls":[
        {"index":0,"id":"call_a","function":{"name":"echo","arguments":"{\"t\""}}]}}]}));
    acc.push(&json!({"choices":[{"delta":{"tool_calls":[
        {"index":0,"function":{"arguments":":1}"}}]}}]}));
    acc.push(&json!({"choices":[{"delta":{},"finish_reason":"tool_calls"}]}));
    acc.push(
        &json!({"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}),
    );

    let response = acc.finish();
    assert_eq!(response.message.content.as_deref(), Some("Hello"));
    assert_eq!(response.message.reasoning.as_deref(), Some("hmm"));
    assert_eq!(response.message.tool_calls.len(), 1);
    assert_eq!(response.message.tool_calls[0].id, "call_a");
    assert_eq!(response.message.tool_calls[0].arguments, "{\"t\":1}");
    assert_eq!(response.finish_reason.as_deref(), Some("tool_calls"));
    assert_eq!(response.usage.total_tokens, 15);
}

/// The minimax fusion bug: parallel calls all streamed at index 0, with
/// id/name re-sent whole per call — must become TWO calls, never
/// "regentregent" with "{...}{...}" arguments.
#[test]
fn parallel_calls_at_the_same_index_stay_separate() {
    let mut acc = StreamAccumulator::default();
    acc.push(&json!({"choices":[{"delta":{"tool_calls":[
        {"index":0,"id":"call_x_1","function":{"name":"regent","arguments":"{\"method\":\"agents.list\"}"}}]}}]}));
    acc.push(&json!({"choices":[{"delta":{"tool_calls":[
        {"index":0,"id":"call_x_2","function":{"name":"regent","arguments":"{\"method\":\"model.get\"}"}}]}}]}));

    let calls = acc.finish().message.tool_calls;
    assert_eq!(calls.len(), 2);
    assert_eq!(
        (calls[0].id.as_str(), calls[0].name.as_str()),
        ("call_x_1", "regent")
    );
    assert_eq!(calls[1].arguments, "{\"method\":\"model.get\"}");
}

/// No `index` at all: id-bearing fragments open calls, id-less fragments
/// continue the LAST call (never spray one call across slots).
#[test]
fn indexless_fragments_continue_the_last_call() {
    let mut acc = StreamAccumulator::default();
    acc.push(&json!({"choices":[{"delta":{"tool_calls":[
        {"id":"call_a","function":{"name":"echo","arguments":"{\"t\""}}]}}]}));
    acc.push(&json!({"choices":[{"delta":{"tool_calls":[
        {"function":{"arguments":":1}"}}]}}]}));

    let calls = acc.finish().message.tool_calls;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments, "{\"t\":1}");
}

#[test]
fn empty_stream_finishes_to_an_empty_assistant() {
    let response = StreamAccumulator::default().finish();
    assert_eq!(response.message.content, None);
    assert!(response.message.tool_calls.is_empty());
}

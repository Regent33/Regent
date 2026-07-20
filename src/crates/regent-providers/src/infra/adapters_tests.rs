//! Unit tests for `adapters` (extracted for the file-size rule; same module
//! tree via #[path] — `use super::*` still sees the parent).

use super::{assistant_message, build_payload, message_to_wire, parse_response};
use crate::domain::entities::ChatRequest;
use regent_kernel::{ChatMessage, ToolCall, ToolDefinition};
use serde_json::json;

// A reasoning-only response is provider output, but not a user answer.
// Keep it private and classify the response as unusable so a configured
// fallback gets a chance before the agent harness performs its last repair.
#[test]
fn reasoning_only_response_stays_private_for_harness_repair() {
    let body = json!({
        "choices": [{
            "message": {
                "content": "",
                "reasoning_content": "I can't pull up a specific song — I have no music tool."
            },
            "finish_reason": "stop"
        }]
    });
    let response = parse_response(&body).unwrap();
    assert!(
        response.is_empty(),
        "reasoning-only output is not user-actionable"
    );
    assert!(
        response
            .message
            .content
            .as_deref()
            .is_some_and(|content| content.trim().is_empty()),
        "private reasoning must not be promoted into visible content"
    );
    assert_eq!(
        response.message.reasoning.as_deref(),
        Some("I can't pull up a specific song — I have no music tool.")
    );
}

// Reasoning + a tool call is a normal agentic turn — the tool call is the
// output; reasoning stays in its own slot, content is untouched.
#[test]
fn reasoning_with_a_tool_call_is_left_untouched() {
    let msg = assistant_message(
        None,
        vec![ToolCall {
            id: "c1".into(),
            name: "play".into(),
            arguments: "{}".into(),
        }],
        Some("thinking about which tool".into()),
    );
    assert!(
        msg.content.is_none(),
        "content stays empty — the tool call answers"
    );
    assert_eq!(msg.reasoning.as_deref(), Some("thinking about which tool"));
    assert_eq!(msg.tool_calls.len(), 1);
}

// A truly empty response (no content, no tools, no reasoning) stays empty
// so the turn's retry/failover still fires.
#[test]
fn a_wholly_empty_response_stays_empty() {
    let msg = assistant_message(None, vec![], None);
    assert!(msg.content.is_none() && msg.tool_calls.is_empty() && msg.reasoning.is_none());
}

// The GLM-via-NIM failure: a model that streamed malformed argument JSON
// poisoned every later request ("invalid tool call arguments", HTTP 400,
// permanently — the bad call rides the replayed history). Replay degrades
// unparseable arguments to "{}"; valid ones pass through byte-identical.
#[test]
fn replay_sanitizes_malformed_tool_call_arguments() {
    let assistant = ChatMessage::assistant(
        None,
        vec![
            ToolCall {
                id: "a".into(),
                name: "read_file".into(),
                arguments: "{\"path\": \"x.rs\"}".into(),
            },
            ToolCall {
                id: "b".into(),
                name: "glob".into(),
                arguments: "{\"pattern\": \"src".into(), // truncated stream
            },
        ],
    );
    let wire = message_to_wire(&assistant);
    assert_eq!(
        wire["tool_calls"][0]["function"]["arguments"],
        "{\"path\": \"x.rs\"}"
    );
    assert_eq!(wire["tool_calls"][1]["function"]["arguments"], "{}");
}

#[test]
fn tool_payload_explicitly_allows_automatic_tool_selection() {
    let request = ChatRequest::new("system", vec![ChatMessage::user("search")]).with_tools(vec![
        ToolDefinition {
            name: "web_search".into(),
            description: "Search the web".into(),
            parameters: json!({"type": "object"}),
            toolset: "web".into(),
        },
    ]);
    let payload = build_payload("model", &request);
    assert_eq!(payload["tool_choice"], "auto");

    let without_tools = build_payload(
        "model",
        &ChatRequest::new("system", vec![ChatMessage::user("hello")]),
    );
    assert!(without_tools.get("tool_choice").is_none());
}

//! Unit tests for `response` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;
use serde_json::json;

#[test]
fn parses_text_tool_use_and_usage() {
    let body = json!({
        "content": [
            {"type": "text", "text": "let me check"},
            {"type": "tool_use", "id": "toolu_9", "name": "grep", "input": {"q": "foo"}}
        ],
        "stop_reason": "tool_use",
        "usage": {"input_tokens": 100, "output_tokens": 20, "cache_read_input_tokens": 50}
    });
    let resp = parse_response(&body).unwrap();
    assert_eq!(resp.message.content.as_deref(), Some("let me check"));
    assert_eq!(resp.message.tool_calls.len(), 1);
    assert_eq!(resp.message.tool_calls[0].name, "grep");
    assert_eq!(resp.message.tool_calls[0].arguments, r#"{"q":"foo"}"#);
    assert_eq!(resp.finish_reason.as_deref(), Some("tool_use"));
    assert_eq!(resp.usage.prompt_tokens, 150); // 100 input + 50 cache read
    assert_eq!(resp.usage.completion_tokens, 20);
    // SPL P2 (deliverable 5b): the cache split is passed through.
    assert_eq!(resp.usage.cache_read_tokens, Some(50));
    assert_eq!(resp.usage.cache_write_tokens, Some(0));
}

// SPL P2 (deliverable 5b): a cache-warming turn maps both split fields.
#[test]
fn maps_anthropic_cache_creation_and_read_fields() {
    let body = json!({
        "content": [{"type": "text", "text": "hi"}],
        "stop_reason": "end_turn",
        "usage": {
            "input_tokens": 12,
            "output_tokens": 7,
            "cache_creation_input_tokens": 2048,
            "cache_read_input_tokens": 900
        }
    });
    let usage = parse_response(&body).unwrap().usage;
    assert_eq!(usage.cache_read_tokens, Some(900));
    assert_eq!(usage.cache_write_tokens, Some(2048));
    // prompt_total rolls up uncached + read + write.
    assert_eq!(usage.prompt_tokens, 12 + 900 + 2048);
}

#[test]
fn refusal_surfaces_a_placeholder() {
    let body = json!({"content": [], "stop_reason": "refusal", "usage": {}});
    let resp = parse_response(&body).unwrap();
    assert_eq!(resp.finish_reason.as_deref(), Some("refusal"));
    assert!(resp.message.content.unwrap().contains("declined"));
}

#[test]
fn thinking_blocks_map_to_reasoning() {
    let body = json!({
        "content": [
            {"type": "thinking", "thinking": "step one"},
            {"type": "text", "text": "answer"}
        ],
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 5, "output_tokens": 5}
    });
    let resp = parse_response(&body).unwrap();
    assert_eq!(resp.message.reasoning.as_deref(), Some("step one"));
    assert_eq!(resp.message.content.as_deref(), Some("answer"));
}

#[test]
fn thinking_block_signature_is_captured_for_replay() {
    let body = json!({
        "content": [
            {"type": "thinking", "thinking": "reason", "signature": "sig-abc"},
            {"type": "text", "text": "answer"}
        ],
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 5, "output_tokens": 5}
    });
    let resp = parse_response(&body).unwrap();
    assert_eq!(resp.message.reasoning.as_deref(), Some("reason"));
    assert_eq!(resp.message.thinking_signature.as_deref(), Some("sig-abc"));
}

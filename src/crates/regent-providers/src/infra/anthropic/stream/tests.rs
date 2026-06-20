use super::*;
use serde_json::json;

#[test]
fn accumulates_text_deltas_in_order() {
    let mut acc = StreamAccumulator::new();
    let mut seen = String::new();
    let events = [
        json!({"type": "message_start", "message": {"usage": {"input_tokens": 30}}}),
        json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text"}}),
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "Hel"}}),
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "lo!"}}),
        json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {"output_tokens": 4}}),
    ];
    for e in &events {
        if let Some(d) = acc.push(e) {
            seen.push_str(&d);
        }
    }
    assert_eq!(seen, "Hello!");
    let resp = acc.finish();
    assert_eq!(resp.message.content.as_deref(), Some("Hello!"));
    assert_eq!(resp.finish_reason.as_deref(), Some("end_turn"));
    assert_eq!(resp.usage.prompt_tokens, 30);
    assert_eq!(resp.usage.completion_tokens, 4);
}

#[test]
fn captures_thinking_text_and_signature() {
    let mut acc = StreamAccumulator::new();
    let events = [
        json!({"type": "content_block_delta", "index": 0,
               "delta": {"type": "thinking_delta", "thinking": "hmm"}}),
        json!({"type": "content_block_delta", "index": 0,
               "delta": {"type": "signature_delta", "signature": "sig-z"}}),
        json!({"type": "content_block_delta", "index": 1,
               "delta": {"type": "text_delta", "text": "ans"}}),
        json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {"output_tokens": 3}}),
    ];
    for e in &events {
        acc.push(e);
    }
    let resp = acc.finish();
    assert_eq!(resp.message.reasoning.as_deref(), Some("hmm"));
    assert_eq!(resp.message.thinking_signature.as_deref(), Some("sig-z"));
    assert_eq!(resp.message.content.as_deref(), Some("ans"));
}

#[test]
fn reassembles_tool_use_from_json_deltas() {
    let mut acc = StreamAccumulator::new();
    let events = [
        json!({"type": "content_block_start", "index": 0,
               "content_block": {"type": "tool_use", "id": "toolu_7", "name": "read_file"}}),
        json!({"type": "content_block_delta", "index": 0,
               "delta": {"type": "input_json_delta", "partial_json": "{\"path\":"}}),
        json!({"type": "content_block_delta", "index": 0,
               "delta": {"type": "input_json_delta", "partial_json": "\"/tmp/x\"}"}}),
        json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"}, "usage": {"output_tokens": 12}}),
    ];
    for e in &events {
        assert!(acc.push(e).is_none(), "tool deltas are not visible text");
    }
    let resp = acc.finish();
    assert_eq!(resp.message.tool_calls.len(), 1);
    assert_eq!(resp.message.tool_calls[0].name, "read_file");
    assert_eq!(resp.message.tool_calls[0].arguments, r#"{"path":"/tmp/x"}"#);
    assert_eq!(resp.finish_reason.as_deref(), Some("tool_use"));
}

//! Anthropic Messages API response parsing (non-streaming): content blocks →
//! the internal `ChatResponse`.

use crate::domain::entities::ChatResponse;
use crate::domain::errors::ProviderError;
use or_core::TokenUsage;
use regent_kernel::{ChatMessage, ToolCall};
use serde_json::Value;

pub fn parse_response(body: &Value) -> Result<ChatResponse, ProviderError> {
    let blocks = body
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| ProviderError::Parse("missing content array".into()))?;

    let mut text = String::new();
    let mut reasoning = String::new();
    let mut signature: Option<String> = None;
    let mut tool_calls: Vec<ToolCall> = Vec::new();

    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(t) = block.get("text").and_then(Value::as_str) {
                    text.push_str(t);
                }
            }
            Some("thinking") => {
                if let Some(t) = block.get("thinking").and_then(Value::as_str) {
                    reasoning.push_str(t);
                }
                // Captured so the block can be replayed verbatim next turn.
                if let Some(s) = block.get("signature").and_then(Value::as_str) {
                    signature = Some(s.to_owned());
                }
            }
            Some("tool_use") => tool_calls.push(parse_tool_use(block)?),
            _ => {}
        }
    }

    let stop_reason = body.get("stop_reason").and_then(Value::as_str);

    // A pre-output refusal carries no text; surface a stable placeholder so
    // the turn isn't silently empty. Otherwise empty text → None (a pure
    // tool-call turn).
    let content = if !text.is_empty() {
        Some(text)
    } else if stop_reason == Some("refusal") {
        Some("(request declined by the safety system)".to_owned())
    } else {
        None
    };

    let mut assistant = ChatMessage::assistant(content, tool_calls);
    if !reasoning.is_empty() {
        assistant.reasoning = Some(reasoning);
    }
    assistant.thinking_signature = signature;

    Ok(ChatResponse {
        message: assistant,
        usage: parse_usage(body.get("usage")),
        finish_reason: stop_reason.map(ToOwned::to_owned),
    })
}

fn parse_tool_use(block: &Value) -> Result<ToolCall, ProviderError> {
    let id = block
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| ProviderError::Parse("tool_use missing id".into()))?;
    let name = block
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| ProviderError::Parse("tool_use missing name".into()))?;
    // Internally arguments are a JSON string; Anthropic sends an object.
    let arguments = block.get("input").map_or_else(|| "{}".to_owned(), ToString::to_string);
    Ok(ToolCall { id: id.to_owned(), name: name.to_owned(), arguments })
}

/// Anthropic reports `input_tokens` net of cached tokens, with cache reads and
/// writes accounted separately. Roll them into the prompt total so context and
/// cost accounting see the full processed prefix.
pub(crate) fn parse_usage(value: Option<&Value>) -> TokenUsage {
    let read = |key: &str| -> u32 {
        value
            .and_then(|u| u.get(key))
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0)
    };
    // Split so the true cost is legible: `input_tokens` is the uncached prefix
    // (full price), `cache_read` is served from the prompt cache (~0.1x), and
    // `cache_write` is the one-time cache seed (~1.25x). `prompt_total` rolls
    // them up for context-window/compaction accounting — but a big total is
    // mostly cache_read on a warm turn, not full-price input.
    let uncached = read("input_tokens");
    let cache_read = read("cache_read_input_tokens");
    let cache_write = read("cache_creation_input_tokens");
    let prompt = uncached + cache_read + cache_write;
    tracing::debug!(
        uncached_input = uncached,
        cache_read,
        cache_write,
        prompt_total = prompt,
        "token usage (prompt_total is the full processed prefix; cache_read is billed ~0.1x)"
    );
    let completion = read("output_tokens");
    TokenUsage {
        prompt_tokens: prompt,
        completion_tokens: completion,
        total_tokens: prompt + completion,
    }
}

#[cfg(test)]
mod tests {
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
}

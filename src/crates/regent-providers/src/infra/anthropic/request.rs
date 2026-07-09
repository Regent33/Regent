//! Anthropic Messages API request building: assembles the payload with
//! prompt-cache breakpoints on the stable tools+system prefix and the
//! extended-thinking control. Transcript translation lives in `messages`.
//!
//! Render order is tools → system → messages, so a single
//! `cache_control: {type:"ephemeral"}` on the last system block (or the last
//! tool when there's no system) caches the whole byte-stable prefix; volatile
//! transcript turns sit after it and never invalidate it.

use super::messages::build_messages;
use crate::domain::entities::ChatRequest;
use serde_json::{Value, json};

/// Anthropic requires `max_tokens`; use this when the request leaves it unset.
const DEFAULT_MAX_TOKENS: u32 = 8192;

pub fn build_payload(model: &str, request: &ChatRequest) -> Value {
    let mut payload = json!({
        "model": model,
        "max_tokens": request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        "messages": build_messages(&request.messages),
    });

    // Tools render first. Cache them with the system block below unless there
    // is no system text, in which case the breakpoint lands on the last tool.
    if !request.tools.is_empty() {
        let last = request.tools.len() - 1;
        let cache_on_tools = request.system.is_empty();
        payload["tools"] = Value::Array(
            request
                .tools
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let mut tool = json!({
                        "name": t.name,
                        "description": t.description,
                        "input_schema": t.parameters,
                    });
                    if cache_on_tools && i == last {
                        tool["cache_control"] = json!({"type": "ephemeral"});
                    }
                    tool
                })
                .collect(),
        );
    }

    // System travels separately as a cacheable text block (the stable prefix).
    if !request.system.is_empty() {
        payload["system"] = json!([{
            "type": "text",
            "text": request.system,
            "cache_control": {"type": "ephemeral"},
        }]);
    }

    // Extended thinking. When enabled, Anthropic forbids a custom temperature
    // (only the default is allowed), so we skip it.
    if let Some(budget) = request.thinking_budget {
        payload["thinking"] = json!({"type": "enabled", "budget_tokens": budget});
    } else if let Some(temperature) = request.temperature {
        payload["temperature"] = json!(temperature);
    }
    payload
}

/// Same as [`build_payload`] but with `"stream": true` for the SSE endpoint.
pub fn build_streaming_payload(model: &str, request: &ChatRequest) -> Value {
    let mut payload = build_payload(model, request);
    payload["stream"] = json!(true);
    payload
}

#[cfg(test)]
mod tests {
    use super::*;
    use regent_kernel::{ChatMessage, ToolDefinition};

    fn tool(name: &str) -> ToolDefinition {
        ToolDefinition {
            name: name.to_owned(),
            description: "d".to_owned(),
            parameters: json!({"type": "object", "properties": {}}),
            toolset: "core".to_owned(),
        }
    }

    #[test]
    fn system_block_carries_the_cache_breakpoint() {
        let req = ChatRequest::new("you are regent", vec![ChatMessage::user("hi")]);
        let payload = build_payload("claude-sonnet-4-6", &req);
        assert_eq!(payload["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(payload["max_tokens"], 8192);
    }

    #[test]
    fn breakpoint_falls_on_last_tool_when_system_is_empty() {
        let req = ChatRequest::new("", vec![ChatMessage::user("hi")])
            .with_tools(vec![tool("a"), tool("b")]);
        let payload = build_payload("claude-sonnet-4-6", &req);
        assert!(payload["tools"][0].get("cache_control").is_none());
        assert_eq!(payload["tools"][1]["cache_control"]["type"], "ephemeral");
        assert!(payload.get("system").is_none());
    }

    #[test]
    fn thinking_budget_enables_thinking_and_drops_temperature() {
        let mut req = ChatRequest::new("s", vec![ChatMessage::user("hi")]).with_thinking(2048);
        req.temperature = Some(0.7);
        let payload = build_payload("m", &req);
        assert_eq!(payload["thinking"]["type"], "enabled");
        assert_eq!(payload["thinking"]["budget_tokens"], 2048);
        assert!(
            payload.get("temperature").is_none(),
            "thinking forbids a custom temperature"
        );
    }

    #[test]
    fn temperature_passes_through_when_thinking_is_off() {
        let mut req = ChatRequest::new("s", vec![ChatMessage::user("hi")]);
        req.temperature = Some(0.3);
        let payload = build_payload("m", &req);
        // f32 → f64 widening, so compare with tolerance.
        assert!((payload["temperature"].as_f64().unwrap() - 0.3).abs() < 1e-6);
        assert!(payload.get("thinking").is_none());
    }

    #[test]
    fn streaming_payload_sets_the_stream_flag() {
        let payload = build_streaming_payload("m", &ChatRequest::new("s", vec![]));
        assert_eq!(payload["stream"], true);
    }
}

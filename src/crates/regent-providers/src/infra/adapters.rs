//! OpenAI chat-completions wire format: payload building and response
//! parsing. Pure functions — unit-testable without a network.

use crate::domain::entities::ChatRequest;
use crate::domain::entities::ChatResponse;
use crate::domain::errors::ProviderError;
use or_core::TokenUsage;
use regent_kernel::{ChatMessage, Role, ToolCall};
use serde_json::{Value, json};

pub fn build_payload(model: &str, request: &ChatRequest) -> Value {
    let mut messages = Vec::with_capacity(request.messages.len() + 1);
    if !request.system.is_empty() {
        messages.push(json!({"role": "system", "content": request.system}));
    }
    for message in &request.messages {
        messages.push(message_to_wire(message));
    }
    let mut payload = json!({"model": model, "messages": messages});
    if !request.tools.is_empty() {
        payload["tools"] = Value::Array(
            request
                .tools
                .iter()
                .map(|t| {
                    json!({"type": "function", "function": {
                        "name": t.name, "description": t.description, "parameters": t.parameters,
                    }})
                })
                .collect(),
        );
    }
    if let Some(temperature) = request.temperature {
        payload["temperature"] = json!(temperature);
    }
    if let Some(max_tokens) = request.max_tokens {
        payload["max_tokens"] = json!(max_tokens);
    }
    payload
}

fn message_to_wire(message: &ChatMessage) -> Value {
    match message.role {
        Role::User => json!({"role": "user", "content": message.content}),
        Role::Assistant => {
            let mut wire = json!({"role": "assistant", "content": message.content});
            if !message.tool_calls.is_empty() {
                wire["tool_calls"] = Value::Array(
                    message
                        .tool_calls
                        .iter()
                        .map(|c| {
                            json!({"id": c.id, "type": "function",
                                   "function": {"name": c.name, "arguments": c.arguments}})
                        })
                        .collect(),
                );
            }
            wire
        }
        Role::Tool => json!({
            "role": "tool",
            "tool_call_id": message.tool_call_id,
            "content": message.content,
        }),
    }
}

pub fn parse_response(body: &Value) -> Result<ChatResponse, ProviderError> {
    let message = body
        .pointer("/choices/0/message")
        .ok_or_else(|| ProviderError::Parse("missing choices[0].message".into()))?;
    let content = message
        .get("content")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let tool_calls = match message.get("tool_calls").and_then(Value::as_array) {
        Some(calls) => calls
            .iter()
            .map(parse_tool_call)
            .collect::<Result<_, _>>()?,
        None => Vec::new(),
    };
    // Providers expose reasoning under different keys; keep the first found.
    let reasoning = ["reasoning_content", "reasoning"]
        .iter()
        .find_map(|key| message.get(*key).and_then(Value::as_str))
        .map(ToOwned::to_owned);
    let mut assistant = ChatMessage::assistant(content, tool_calls);
    assistant.reasoning = reasoning;
    Ok(ChatResponse {
        message: assistant,
        usage: parse_usage(body.get("usage")),
        finish_reason: body
            .pointer("/choices/0/finish_reason")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
    })
}

fn parse_tool_call(value: &Value) -> Result<ToolCall, ProviderError> {
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| ProviderError::Parse("tool call missing id".into()))?;
    let function = value
        .get("function")
        .ok_or_else(|| ProviderError::Parse("tool call missing function".into()))?;
    let name = function
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| ProviderError::Parse("tool call missing function.name".into()))?;
    let arguments = match function.get("arguments") {
        Some(Value::String(s)) => s.clone(),
        // Some providers send arguments as an object instead of a string.
        Some(other) => other.to_string(),
        None => "{}".to_owned(),
    };
    Ok(ToolCall {
        id: id.to_owned(),
        name: name.to_owned(),
        arguments,
    })
}

fn parse_usage(value: Option<&Value>) -> TokenUsage {
    let read = |key: &str| -> u32 {
        value
            .and_then(|u| u.get(key))
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0)
    };
    TokenUsage {
        prompt_tokens: read("prompt_tokens"),
        completion_tokens: read("completion_tokens"),
        total_tokens: read("total_tokens"),
        // SPL P2: OpenAI-compatible providers report cache hits under
        // `prompt_tokens_details.cached_tokens` (implicit caching). Map it to
        // cache_read; `None` when absent so non-caching providers stay unchanged.
        // There is no separate cache-write field on this shape.
        cache_read_tokens: cached_tokens(value),
        cache_write_tokens: None,
    }
}

/// Extracts `prompt_tokens_details.cached_tokens` from an OpenAI-style usage
/// object, if present (the implicit-cache read count).
fn cached_tokens(value: Option<&Value>) -> Option<u32> {
    value
        .and_then(|u| u.get("prompt_tokens_details"))
        .and_then(|d| d.get("cached_tokens"))
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
}

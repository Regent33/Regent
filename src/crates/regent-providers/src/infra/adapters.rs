//! OpenAI chat-completions wire format: payload building and response
//! parsing. Pure functions — unit-testable without a network.

use crate::domain::entities::ChatRequest;
use crate::domain::entities::ChatResponse;
use crate::domain::errors::ProviderError;
use or_core::TokenUsage;
use regent_kernel::{ChatMessage, Role, ToolCall};
use serde_json::{Value, json};

/// Assembles the final assistant message, surfacing reasoning as the answer
/// when the model produced ONLY reasoning — empty content AND no tool calls.
/// Reasoning models (nemotron-3-ultra, glm-5.2, deepseek-r1, …) sometimes
/// answer entirely in the thinking channel and leave `content` empty; without
/// this the turn looks empty, the chain fails over to the next reasoning model,
/// and IT does the same — an all-empty cascade over a response that actually
/// exists. Promoting keeps the reasoning as the reply (cleared from the
/// thinking slot so it isn't shown twice). A reasoning + tool_call turn is
/// left untouched — the tool call is the real output there.
pub fn assistant_message(
    content: Option<String>,
    tool_calls: Vec<ToolCall>,
    reasoning: Option<String>,
) -> ChatMessage {
    let content_empty = content.as_deref().is_none_or(|c| c.trim().is_empty());
    if content_empty
        && tool_calls.is_empty()
        && let Some(thought) = reasoning.as_deref().filter(|r| !r.trim().is_empty())
    {
        return ChatMessage::assistant(Some(thought.to_owned()), tool_calls);
    }
    let mut message = ChatMessage::assistant(content, tool_calls);
    message.reasoning = reasoning;
    message
}

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
                            // Replay-sanitize: a model that streamed malformed
                            // argument JSON (GLM via NIM has) would otherwise
                            // poison EVERY later request in the session — the
                            // provider 400s ("invalid tool call arguments") on
                            // the replayed history, permanently. The tool
                            // already ran; replayed args are informational,
                            // so an unparseable string degrades to "{}".
                            let arguments = if serde_json::from_str::<Value>(&c.arguments).is_ok() {
                                c.arguments.clone()
                            } else {
                                tracing::warn!(
                                    tool = %c.name,
                                    "replacing malformed tool-call arguments on replay"
                                );
                                "{}".to_owned()
                            };
                            json!({"id": c.id, "type": "function",
                                   "function": {"name": c.name, "arguments": arguments}})
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
    Ok(ChatResponse {
        message: assistant_message(content, tool_calls, reasoning),
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

#[cfg(test)]
mod tests {
    use super::{assistant_message, message_to_wire, parse_response};
    use regent_kernel::{ChatMessage, ToolCall};
    use serde_json::json;

    // A reasoning model that answered ONLY in its thinking channel (empty
    // content, no tool calls) must not look empty — the reasoning becomes the
    // reply, so the chain returns it instead of cascading to the next model.
    #[test]
    fn reasoning_only_response_surfaces_reasoning_as_the_answer() {
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
        assert!(!response.is_empty(), "a reasoning-only answer is NOT empty");
        assert_eq!(
            response.message.content.as_deref(),
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
        assert!(msg.content.is_none(), "content stays empty — the tool call answers");
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
}

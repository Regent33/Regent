//! SSE streaming for the OpenAI-compatible chat endpoint. Same wire shapes as
//! `adapters.rs`, delivered incrementally: `delta.content` fragments reach the
//! sink as they arrive, tool-call fragments accumulate by index, and the final
//! `ChatResponse` matches what `parse_response` would produce non-streaming.

use crate::domain::contracts::DeltaSink;
use crate::domain::entities::{ChatRequest, ChatResponse};
use crate::domain::errors::ProviderError;
use crate::infra::adapters::build_payload;
use crate::infra::http::truncate;
use crate::infra::openai_compat::OpenAiCompatChatConfig;
use futures::StreamExt;
use or_core::TokenUsage;
use regent_kernel::{ChatMessage, ToolCall};
use serde_json::Value;

/// Accumulates OpenAI stream chunks into a final response. Pure — unit-testable
/// without a network. `push` returns the chunk's content fragment, if any.
#[derive(Default)]
pub(super) struct StreamAccumulator {
    content: String,
    reasoning: String,
    tools: Vec<(String, String, String)>, // (id, name, arguments) by index
    finish_reason: Option<String>,
    usage: TokenUsage,
}

impl StreamAccumulator {
    pub(super) fn push(&mut self, chunk: &Value) -> Option<String> {
        // Usage rides the final chunk (stream_options.include_usage), often
        // with an empty choices array.
        if let Some(usage) = chunk.get("usage").filter(|u| !u.is_null()) {
            let read = |key: &str| {
                usage
                    .get(key)
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok())
                    .unwrap_or(0)
            };
            self.usage = TokenUsage {
                prompt_tokens: read("prompt_tokens"),
                completion_tokens: read("completion_tokens"),
                total_tokens: read("total_tokens"),
                // SPL P2: implicit-cache read count, when the provider reports it.
                cache_read_tokens: usage
                    .get("prompt_tokens_details")
                    .and_then(|d| d.get("cached_tokens"))
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok()),
                cache_write_tokens: None,
            };
        }
        if let Some(reason) = chunk
            .pointer("/choices/0/finish_reason")
            .and_then(Value::as_str)
        {
            self.finish_reason = Some(reason.to_owned());
        }
        let delta = chunk.pointer("/choices/0/delta")?;
        for key in ["reasoning_content", "reasoning"] {
            if let Some(r) = delta.get(key).and_then(Value::as_str) {
                self.reasoning.push_str(r);
            }
        }
        if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for call in calls {
                let id = call.get("id").and_then(Value::as_str).unwrap_or("");
                // Slot resolution has to survive buggy providers (minimax et
                // al.) that stream EVERY parallel call at index 0 and re-send
                // id/name per fragment — keying on index alone fused calls
                // into one ("regentregent", "{...}{...}"). A fragment whose id
                // differs from its slot's id is a NEW call.
                let index = match call.get("index").and_then(Value::as_u64) {
                    Some(n) => {
                        let n = n as usize;
                        match self.tools.get(n) {
                            Some(slot) if !id.is_empty() && !slot.0.is_empty() && slot.0 != id => {
                                self.tools.len()
                            }
                            _ => n,
                        }
                    }
                    // No index: an id matches its call's slot; a fresh id (or
                    // an id-less first fragment) starts a new call; id-less
                    // continuations belong to the last slot.
                    None if !id.is_empty() => self
                        .tools
                        .iter()
                        .position(|s| s.0 == id)
                        .unwrap_or(self.tools.len()),
                    None => self.tools.len().saturating_sub(1),
                };
                while self.tools.len() <= index {
                    self.tools.push(Default::default());
                }
                let slot = &mut self.tools[index];
                // id/name arrive whole (re-sent per fragment by some
                // providers) — set once; only arguments accumulate.
                if slot.0.is_empty() {
                    slot.0.push_str(id);
                }
                if let Some(name) = call.pointer("/function/name").and_then(Value::as_str)
                    && slot.1.is_empty()
                {
                    slot.1.push_str(name);
                }
                if let Some(args) = call.pointer("/function/arguments").and_then(Value::as_str) {
                    slot.2.push_str(args);
                }
            }
        }
        let fragment = delta.get("content").and_then(Value::as_str)?;
        if fragment.is_empty() {
            return None;
        }
        self.content.push_str(fragment);
        Some(fragment.to_owned())
    }

    pub(super) fn finish(self) -> ChatResponse {
        let tool_calls: Vec<ToolCall> = self
            .tools
            .into_iter()
            .enumerate()
            .filter(|(_, (_, name, _))| !name.is_empty())
            .map(|(i, (id, name, arguments))| ToolCall {
                // A provider that omits ids still needs one — results are
                // matched back by id.
                id: if id.is_empty() {
                    format!("call_{i}")
                } else {
                    id
                },
                name,
                arguments: if arguments.is_empty() {
                    "{}".to_owned()
                } else {
                    arguments
                },
            })
            .collect();
        let content = (!self.content.is_empty()).then_some(self.content);
        let mut message = ChatMessage::assistant(content, tool_calls);
        message.reasoning = (!self.reasoning.is_empty()).then_some(self.reasoning);
        ChatResponse {
            message,
            usage: self.usage,
            finish_reason: self.finish_reason,
        }
    }
}

/// One streaming attempt: open the SSE stream, feed each `data:` line to the
/// accumulator, forward content fragments to `on_delta`. No retry — a partial
/// stream can't be replayed without double-emitting deltas.
pub(super) async fn stream_once(
    client: &reqwest::Client,
    config: &OpenAiCompatChatConfig,
    request: &ChatRequest,
    on_delta: DeltaSink<'_>,
) -> Result<ChatResponse, ProviderError> {
    let url = format!(
        "{}{}",
        config.base_url.trim_end_matches('/'),
        config.api_path
    );
    let mut payload = build_payload(&config.model, request);
    payload["stream"] = Value::Bool(true);
    payload["stream_options"] = serde_json::json!({"include_usage": true});
    let response = client
        .post(&url)
        .bearer_auth(&config.api_key)
        .timeout(config.timeout)
        .json(&payload)
        .send()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;
    let status = response.status().as_u16();
    if !(200..=299).contains(&status) {
        let body = response.text().await.unwrap_or_default();
        return Err(match status {
            401 | 403 => ProviderError::Auth { status },
            429 => ProviderError::RateLimited,
            // Redact before surfacing — an error body can echo our key.
            _ => ProviderError::Api {
                status,
                body: truncate(&regent_kernel::redact_secrets(&body), 600),
            },
        });
    }

    let mut stream = response.bytes_stream();
    let mut buf = String::new();
    let mut acc = StreamAccumulator::default();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| ProviderError::Network(e.to_string()))?;
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(nl) = buf.find('\n') {
            let line: String = buf.drain(..=nl).collect();
            let Some(data) = line.trim_end().strip_prefix("data: ") else {
                continue;
            };
            if data == "[DONE]" {
                continue;
            }
            if let Ok(event) = serde_json::from_str::<Value>(data)
                && let Some(fragment) = acc.push(&event)
            {
                on_delta(&fragment);
            }
        }
    }
    Ok(acc.finish())
}

#[cfg(test)]
mod tests {
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
        acc.push(&json!({"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}));

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
        assert_eq!((calls[0].id.as_str(), calls[0].name.as_str()), ("call_x_1", "regent"));
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
}

//! Anthropic Messages API streaming: accumulates SSE events into a
//! `ChatResponse`. The HTTP layer handles SSE framing and calls `push` per
//! event; this struct is pure so the accumulation logic is unit-testable
//! without a socket.

use crate::domain::entities::ChatResponse;
use or_core::TokenUsage;
use regent_kernel::{ChatMessage, ToolCall};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Default)]
pub struct StreamAccumulator {
    text: String,
    reasoning: String,
    /// Extended-thinking block signature (arrives as a `signature_delta`).
    signature: Option<String>,
    /// tool_use blocks keyed by content-block index (preserves call order).
    tools: BTreeMap<usize, ToolBuilder>,
    input_tokens: u32,
    cache_read: u32,
    cache_creation: u32,
    output_tokens: u32,
    stop_reason: Option<String>,
}

#[derive(Default)]
struct ToolBuilder {
    id: String,
    name: String,
    json: String,
}

impl StreamAccumulator {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one parsed SSE event. Returns the visible text fragment when the
    /// event is a `text_delta`, so the caller can forward it to a delta sink.
    pub fn push(&mut self, event: &Value) -> Option<String> {
        match event.get("type").and_then(Value::as_str) {
            Some("message_start") => {
                if let Some(u) = event.pointer("/message/usage") {
                    self.input_tokens = read(u, "input_tokens");
                    self.cache_read = read(u, "cache_read_input_tokens");
                    self.cache_creation = read(u, "cache_creation_input_tokens");
                }
                None
            }
            Some("content_block_start") => {
                if let Some(block) = event.get("content_block")
                    && block.get("type").and_then(Value::as_str) == Some("tool_use")
                {
                    self.tools.insert(
                        index_of(event),
                        ToolBuilder {
                            id: block.get("id").and_then(Value::as_str).unwrap_or("").to_owned(),
                            name: block.get("name").and_then(Value::as_str).unwrap_or("").to_owned(),
                            json: String::new(),
                        },
                    );
                }
                None
            }
            Some("content_block_delta") => self.push_delta(event),
            Some("message_delta") => {
                if let Some(sr) = event.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    self.stop_reason = Some(sr.to_owned());
                }
                if let Some(u) = event.get("usage") {
                    self.output_tokens = read(u, "output_tokens");
                }
                None
            }
            _ => None,
        }
    }

    fn push_delta(&mut self, event: &Value) -> Option<String> {
        let delta = event.get("delta")?;
        match delta.get("type").and_then(Value::as_str) {
            Some("text_delta") => {
                let t = delta.get("text").and_then(Value::as_str)?;
                self.text.push_str(t);
                Some(t.to_owned())
            }
            Some("thinking_delta") => {
                if let Some(t) = delta.get("thinking").and_then(Value::as_str) {
                    self.reasoning.push_str(t);
                }
                None
            }
            Some("signature_delta") => {
                if let Some(s) = delta.get("signature").and_then(Value::as_str) {
                    self.signature = Some(s.to_owned());
                }
                None
            }
            Some("input_json_delta") => {
                if let (Some(tb), Some(frag)) = (
                    self.tools.get_mut(&index_of(event)),
                    delta.get("partial_json").and_then(Value::as_str),
                ) {
                    tb.json.push_str(frag);
                }
                None
            }
            _ => None,
        }
    }

    /// Assembles the final response once the stream ends.
    #[must_use]
    pub fn finish(self) -> ChatResponse {
        let content = if !self.text.is_empty() {
            Some(self.text)
        } else if self.stop_reason.as_deref() == Some("refusal") {
            Some("(request declined by the safety system)".to_owned())
        } else {
            None
        };
        let tool_calls: Vec<ToolCall> = self
            .tools
            .into_values()
            .map(|tb| ToolCall {
                id: tb.id,
                name: tb.name,
                arguments: if tb.json.is_empty() { "{}".to_owned() } else { tb.json },
            })
            .collect();
        let mut assistant = ChatMessage::assistant(content, tool_calls);
        if !self.reasoning.is_empty() {
            assistant.reasoning = Some(self.reasoning);
        }
        assistant.thinking_signature = self.signature;
        let prompt = self.input_tokens + self.cache_read + self.cache_creation;
        ChatResponse {
            message: assistant,
            usage: TokenUsage {
                prompt_tokens: prompt,
                completion_tokens: self.output_tokens,
                total_tokens: prompt + self.output_tokens,
            },
            finish_reason: self.stop_reason,
        }
    }
}

fn read(v: &Value, key: &str) -> u32 {
    v.get(key).and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()).unwrap_or(0)
}

fn index_of(event: &Value) -> usize {
    event
        .get("index")
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;

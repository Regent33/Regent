//! Pure OpenAI SSE chunk accumulation.

use crate::domain::entities::ChatResponse;
use or_core::TokenUsage;
use regent_kernel::ToolCall;
use serde_json::Value;

#[derive(Default)]
pub(super) struct StreamAccumulator {
    content: String,
    reasoning: String,
    tools: Vec<(String, String, String)>,
    finish_reason: Option<String>,
    usage: TokenUsage,
}

impl StreamAccumulator {
    pub(super) fn push(&mut self, chunk: &Value) -> Option<String> {
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
            if let Some(reasoning) = delta.get(key).and_then(Value::as_str) {
                self.reasoning.push_str(reasoning);
            }
        }
        if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for call in calls {
                self.push_tool_call(call);
            }
        }
        let fragment = delta.get("content").and_then(Value::as_str)?;
        if fragment.is_empty() {
            return None;
        }
        self.content.push_str(fragment);
        Some(fragment.to_owned())
    }

    fn push_tool_call(&mut self, call: &Value) {
        let id = call.get("id").and_then(Value::as_str).unwrap_or("");
        // Some providers reuse index 0 for every parallel call. A new id means a
        // new slot; id-less continuations belong to the last slot.
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
            None if !id.is_empty() => self
                .tools
                .iter()
                .position(|slot| slot.0 == id)
                .unwrap_or(self.tools.len()),
            None => self.tools.len().saturating_sub(1),
        };
        while self.tools.len() <= index {
            self.tools.push(Default::default());
        }
        let slot = &mut self.tools[index];
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

    pub(super) fn finish(self) -> ChatResponse {
        let tool_calls = self
            .tools
            .into_iter()
            .enumerate()
            .filter(|(_, (_, name, _))| !name.is_empty())
            .map(|(i, (id, name, arguments))| ToolCall {
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
        let reasoning = (!self.reasoning.is_empty()).then_some(self.reasoning);
        ChatResponse {
            message: crate::infra::adapters::assistant_message(content, tool_calls, reasoning),
            usage: self.usage,
            finish_reason: self.finish_reason,
        }
    }
}

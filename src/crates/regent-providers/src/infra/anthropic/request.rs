//! Anthropic Messages API request building: assembles the payload, optionally
//! with prompt-cache `cache_control` breakpoints (SPL P2), and the
//! extended-thinking control. Transcript translation lives in `messages`.
//!
//! Render order is tools → system → messages. When the request carries a
//! `CachePolicy`, up to three breakpoints go on the stable prefix: the LAST
//! tool definition (caches the whole tool block, which renders first), the
//! system block, and the last history message BEFORE the current user turn
//! (caches history through the prior turn). The newest user turn sits after
//! them and never invalidates them. FAIL-OPEN by construction: no policy →
//! no breakpoints (today's behavior), and every placement is infallible JSON
//! insertion, so caching can never break a request.

use super::messages::build_messages;
use crate::domain::entities::{CachePolicy, CacheTtl, ChatRequest};
use serde_json::{Value, json};

/// Anthropic requires `max_tokens`; use this when the request leaves it unset.
const DEFAULT_MAX_TOKENS: u32 = 8192;

/// The `cache_control` object for a policy. 5m is the implicit ephemeral TTL
/// (no `ttl` field); 1h uses Anthropic's documented `"ttl":"1h"` (prompt
/// caching, incl. the 1h TTL, is GA on the first-party API — no beta header).
fn cache_control(policy: CachePolicy) -> Value {
    match policy.ttl {
        CacheTtl::FiveMinutes => json!({"type": "ephemeral"}),
        CacheTtl::OneHour => json!({"type": "ephemeral", "ttl": "1h"}),
    }
}

/// Marks the last content block of the last *history* message (the turn before
/// the current user turn) with `cache_control`, caching the transcript through
/// the prior turn. No-op when there are fewer than two message-turns (the very
/// first turn has no history to cache) — pure and fail-open.
fn mark_history_breakpoint(messages: &mut Value, cc: &Value) {
    let Some(turns) = messages.as_array_mut() else {
        return;
    };
    if turns.len() < 2 {
        return;
    }
    let idx = turns.len() - 2;
    if let Some(blocks) = turns[idx].get_mut("content").and_then(Value::as_array_mut)
        && let Some(last) = blocks.last_mut()
        && let Some(obj) = last.as_object_mut()
    {
        obj.insert("cache_control".to_owned(), cc.clone());
    }
}

pub fn build_payload(model: &str, request: &ChatRequest) -> Value {
    let cc = request.cache.map(cache_control);

    let mut messages = build_messages(&request.messages);
    if let Some(cc) = &cc {
        mark_history_breakpoint(&mut messages, cc);
    }

    let mut payload = json!({
        "model": model,
        "max_tokens": request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        "messages": messages,
    });

    // Tools render first; the breakpoint on the LAST tool caches the whole tool
    // block (its own cache tier). Only when a policy is present.
    if !request.tools.is_empty() {
        let last = request.tools.len() - 1;
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
                    if let Some(cc) = &cc
                        && i == last
                    {
                        tool["cache_control"] = cc.clone();
                    }
                    tool
                })
                .collect(),
        );
    }

    // System travels separately as a cacheable text block (the stable prefix).
    if !request.system.is_empty() {
        let mut block = json!({"type": "text", "text": request.system});
        if let Some(cc) = &cc {
            block["cache_control"] = cc.clone();
        }
        payload["system"] = json!([block]);
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
#[path = "request_tests.rs"]
mod tests;

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
mod tests {
    use super::*;
    use crate::domain::entities::CachePolicy;
    use regent_kernel::{ChatMessage, ToolDefinition};

    fn tool(name: &str) -> ToolDefinition {
        ToolDefinition {
            name: name.to_owned(),
            description: "d".to_owned(),
            parameters: json!({"type": "object", "properties": {}}),
            toolset: "core".to_owned(),
        }
    }

    /// A prior exchange plus a fresh user turn — so there IS a history message
    /// before the current user turn to carry the third breakpoint.
    fn history() -> Vec<ChatMessage> {
        vec![
            ChatMessage::user("first"),
            ChatMessage::assistant(Some("reply".into()), vec![]),
            ChatMessage::user("second"),
        ]
    }

    // Deliverable 5(a): all three breakpoints present when a policy is on.
    #[test]
    fn policy_on_places_breakpoints_at_all_three_positions() {
        let req = ChatRequest::new("you are regent", history())
            .with_tools(vec![tool("a"), tool("b")])
            .with_cache(CachePolicy {
                ttl: CacheTtl::FiveMinutes,
            });
        let payload = build_payload("claude-sonnet-4-6", &req);

        // 1) last tool definition (not earlier ones)
        assert!(payload["tools"][0].get("cache_control").is_none());
        assert_eq!(payload["tools"][1]["cache_control"]["type"], "ephemeral");
        // 2) the system block
        assert_eq!(payload["system"][0]["cache_control"]["type"], "ephemeral");
        // 3) last history message before the current user turn: built messages
        // are [user(first)+.., assistant(reply), user(second)] → the assistant
        // turn at index len-2 carries it; the newest user turn does not.
        let turns = payload["messages"].as_array().unwrap();
        let hist = &turns[turns.len() - 2];
        let last_block = hist["content"].as_array().unwrap().last().unwrap();
        assert_eq!(last_block["cache_control"]["type"], "ephemeral");
        assert!(
            turns
                .last()
                .unwrap()
                .get("content")
                .unwrap()
                .as_array()
                .unwrap()
                .last()
                .unwrap()
                .get("cache_control")
                .is_none(),
            "the current user turn must stay uncached"
        );
    }

    // Deliverable 5(a): NONE when the policy is off (default) — today's behavior.
    #[test]
    fn policy_off_places_no_breakpoints_anywhere() {
        let req = ChatRequest::new("you are regent", history()).with_tools(vec![tool("a")]);
        let payload = build_payload("claude-sonnet-4-6", &req);
        assert!(payload["tools"][0].get("cache_control").is_none());
        assert!(payload["system"][0].get("cache_control").is_none());
        for turn in payload["messages"].as_array().unwrap() {
            for block in turn["content"].as_array().unwrap() {
                assert!(
                    block.get("cache_control").is_none(),
                    "no breakpoints without a policy"
                );
            }
        }
    }

    // 1h TTL uses Anthropic's documented `ttl` field.
    #[test]
    fn one_hour_policy_emits_the_ttl_field() {
        let req = ChatRequest::new("s", vec![ChatMessage::user("hi")]).with_cache(CachePolicy {
            ttl: CacheTtl::OneHour,
        });
        let payload = build_payload("claude-sonnet-4-6", &req);
        assert_eq!(payload["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(payload["system"][0]["cache_control"]["ttl"], "1h");
    }

    // A single-turn session (first turn) has no history breakpoint to place.
    #[test]
    fn first_turn_has_no_history_breakpoint() {
        let req = ChatRequest::new("s", vec![ChatMessage::user("hi")]).with_cache(CachePolicy {
            ttl: CacheTtl::FiveMinutes,
        });
        let payload = build_payload("claude-sonnet-4-6", &req);
        let turns = payload["messages"].as_array().unwrap();
        assert_eq!(turns.len(), 1);
        assert!(turns[0]["content"][0].get("cache_control").is_none());
        // System still cached (the fixed prefix is worth caching from turn one).
        assert_eq!(payload["system"][0]["cache_control"]["type"], "ephemeral");
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

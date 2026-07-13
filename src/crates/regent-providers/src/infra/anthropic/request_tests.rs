//! Unit tests for `request` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

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

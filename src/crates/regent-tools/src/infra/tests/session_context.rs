//! W11 end-to-end: `session_search` returns the turns around a hit.
//!
//! Split from `session_tools.rs` (file-size rule), and separate on purpose —
//! the tests there exercise the pure clamps, and every one of them would pass
//! with the context wiring deleted. These drive the tool itself.

use super::*;
use crate::{ApprovalDecision, ApprovalHandler};
use async_trait::async_trait;
use regent_kernel::{ChatMessage, SessionId};
use std::sync::Arc;

struct AlwaysAllow;

#[async_trait]
impl ApprovalHandler for AlwaysAllow {
    async fn request(&self, _tool: &str, _subject: &str, _why: &str) -> ApprovalDecision {
        ApprovalDecision::Approve
    }
}

/// The shape the feature exists for: the hit is "yes, do that", which answers a
/// question it does not contain.
fn store_with_a_terse_agreement() -> Arc<Store> {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let session = SessionId::generate();
    store
        .create_session(&session, "cli", None, None, None)
        .unwrap();
    for message in [
        ChatMessage::user("should we cap the failover at two hops?"),
        ChatMessage::assistant(Some("That bounds the amplification, yes.".into()), vec![]),
        ChatMessage::user("yes, do that"),
        ChatMessage::assistant(Some("Capped at two hops.".into()), vec![]),
    ] {
        store
            .append_message(&session, &message, None, None)
            .unwrap();
    }
    store
}

async fn search(store: &Arc<Store>, args: Value) -> Value {
    let tool = SessionSearchTool {
        store: Arc::clone(store),
    };
    let ctx = ToolContext::new(std::path::PathBuf::from("."), Arc::new(AlwaysAllow) as _);
    serde_json::from_str(&tool.execute(args, &ctx).await.unwrap()).unwrap()
}

/// The load-bearing one. Searching the terse turn must come back with enough
/// to know what was agreed to.
#[tokio::test]
async fn a_hit_arrives_with_the_turn_that_gives_it_meaning() {
    let store = store_with_a_terse_agreement();
    let out = search(&store, json!({"query": "\"yes, do that\""})).await;

    let hit = &out["results"][0];
    // FTS wraps each matched term individually, so the snippet reads
    // ">>>do<<< >>>that<<<" — strip the markers before asserting on it.
    let snippet = hit["snippet"]
        .as_str()
        .unwrap()
        .replace(">>>", "")
        .replace("<<<", "");
    assert!(
        snippet.contains("do that"),
        "fixture must hit the terse turn: {out}"
    );
    let context = hit["context"].as_array().expect("context is attached");
    let joined: String = context
        .iter()
        .map(|t| t["text"].as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(
        joined.contains("bounds the amplification"),
        "the preceding turn is what makes the hit legible: {joined}"
    );
    // Signed by distance, so the exchange can be reconstructed in order.
    assert_eq!(context[0]["offset"].as_i64(), Some(-1));
    assert!(context[0]["role"].is_string());
}

/// `context: 0` restores the exact pre-W11 payload. A caller paying for 20
/// hits must be able to decline the extra turns.
#[tokio::test]
async fn context_zero_returns_hits_only() {
    let store = store_with_a_terse_agreement();
    let out = search(&store, json!({"query": "\"yes, do that\"", "context": 0})).await;

    let hit = &out["results"][0];
    assert!(hit["snippet"].is_string());
    assert!(
        hit.get("context").is_none(),
        "no empty key on the payload: {hit}"
    );
}

/// The radius is bounded. Context is charged per hit against a result that
/// already allows 20 of them.
#[tokio::test]
async fn an_oversized_context_request_is_clamped() {
    let store = store_with_a_terse_agreement();
    let out = search(&store, json!({"query": "\"yes, do that\"", "context": 99})).await;

    let context = out["results"][0]["context"].as_array().unwrap();
    assert!(
        context.len() <= 2 * CONTEXT_RADIUS_MAX as usize,
        "radius must clamp to {CONTEXT_RADIUS_MAX}: got {} turns",
        context.len()
    );
}

/// Context turns are capped tighter than snippets — they exist to make the hit
/// legible, not to be read in full.
#[tokio::test]
async fn a_long_neighbouring_turn_is_truncated() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let session = SessionId::generate();
    store
        .create_session(&session, "cli", None, None, None)
        .unwrap();
    for message in [
        ChatMessage::assistant(Some("x".repeat(5_000)), vec![]),
        ChatMessage::user("acknowledged the enormous explanation"),
    ] {
        store
            .append_message(&session, &message, None, None)
            .unwrap();
    }

    let out = search(&store, json!({"query": "acknowledged"})).await;
    let text = out["results"][0]["context"][0]["text"].as_str().unwrap();
    assert_eq!(text.chars().count(), CONTEXT_MAX_CHARS + 1, "+1 ellipsis");
    assert!(text.ends_with('…'));
}

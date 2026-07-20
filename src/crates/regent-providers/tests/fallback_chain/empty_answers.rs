//! Empty-but-200 answers: an unusable completion reroutes exactly like a
//! failed one, and a wholly-empty chain hands the emptiness to the turn loop.

use crate::{Flaky, request};
use regent_providers::{ChatProvider, FallbackChat};

#[tokio::test]
async fn empty_200_response_fails_over_to_the_next_provider() {
    // The nemotron case: HTTP 200 but nothing usable (whitespace, no tools).
    // The chain must treat it as a failure and reroute, not hand back a dead
    // turn — this is what makes the "empty response" surface into a failover.
    let empty = Flaky::empty("nemotron");
    let healthy = Flaky::healthy("glm");
    let chain = FallbackChat::new(vec![empty.clone(), healthy.clone()]).unwrap();

    let response = chain.complete(&request()).await.unwrap();
    assert!(response.message.content.unwrap().contains("glm"));
    assert_eq!(empty.calls(), 1, "empty primary attempted once");
    assert_eq!(healthy.calls(), 1, "fallback served the real answer");
}

#[tokio::test]
async fn private_reasoning_without_an_answer_fails_over() {
    let reasoning = Flaky::reasoning_only("nemotron");
    let healthy = Flaky::healthy("glm");
    let chain = FallbackChat::new(vec![reasoning.clone(), healthy.clone()]).unwrap();

    let response = chain.complete(&request()).await.unwrap();
    assert!(response.message.content.unwrap().contains("glm"));
    assert_eq!(
        reasoning.calls(),
        1,
        "reasoning-only primary attempted once"
    );
    assert_eq!(healthy.calls(), 1, "fallback served the usable answer");
}

#[tokio::test]
async fn whole_chain_empty_returns_the_empty_response_for_the_turn_to_retry() {
    // Every member empty → the chain has nothing better; it returns the last
    // empty Ok (NOT an error), so the agent turn loop applies its retry-once-
    // then-surface policy rather than the chain masking it.
    let a = Flaky::empty("a");
    let b = Flaky::empty("b");
    let chain = FallbackChat::new(vec![a.clone(), b.clone()]).unwrap();

    let response = chain.complete(&request()).await.unwrap();
    assert!(
        response.is_empty(),
        "the terminal empty answer reaches the caller"
    );
    assert_eq!(a.calls(), 1);
    assert_eq!(b.calls(), 1, "both members were tried before giving up");
}

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
async fn private_reasoning_without_an_answer_stays_on_the_same_provider() {
    // REVERSES the earlier rule (which failed over here). A reasoning model
    // that thinks and stops short of visible text is not a sick provider: it
    // answered, streamed, and billed. The agent repairs that state in place
    // (reveal-on-stuck, added after this rule) — and the chain is STICKY, so
    // rerouting moved the user off their chosen model for the whole rest of
    // the session while nothing was wrong with it. Only silence is a fault.
    let reasoning = Flaky::reasoning_only("nemotron");
    let healthy = Flaky::healthy("glm");
    let chain = FallbackChat::new(vec![reasoning.clone(), healthy.clone()]).unwrap();

    let response = chain.complete(&request()).await.unwrap();
    assert!(response.message.content.is_none());
    assert!(
        response.message.reasoning.is_some(),
        "the thinking survives"
    );
    assert_eq!(reasoning.calls(), 1, "the chosen model answered");
    assert_eq!(healthy.calls(), 0, "the fallback was never woken");
}

#[tokio::test]
async fn a_stream_cut_before_its_finish_reason_reroutes() {
    // The other side of the rule above. A severed stream also arrives as
    // thinking with no visible answer, but it is NOT the alive-and-well case:
    // the provider hung up mid-thought, so `finish_reason` never came. Told
    // apart only by that terminal chunk, it stayed put here — and the agent
    // then spent a second full-length call on the same dead endpoint before
    // giving up with "empty response ... twice".
    let cut = Flaky::truncated("nemotron");
    let healthy = Flaky::healthy("minimax");
    let chain = FallbackChat::new(vec![cut.clone(), healthy.clone()]).unwrap();

    let response = chain.complete(&request()).await.unwrap();
    assert!(
        response.message.content.unwrap().contains("minimax"),
        "the truncated call rerouted instead of returning its dead thinking"
    );
    assert_eq!(cut.calls(), 1, "the cut provider was attempted once");
    assert_eq!(healthy.calls(), 1, "the fallback served the real answer");
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

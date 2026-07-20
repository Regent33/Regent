//! Auth + pairing round-trip and the /stop busy-guard bypass.

use crate::{EchoHandler, MockAdapter, SleepyHandler, allow, event, settle};
use regent_gateway::{ApprovalRouter, GatewayRunner, RateLimiter};
use std::sync::Arc;

#[tokio::test]
async fn round_trip_with_auth_and_pairing() {
    let adapter = Arc::new(MockAdapter::default());
    let runner = GatewayRunner::new(
        adapter.clone(),
        Arc::new(EchoHandler),
        allow(&["alice"]),
        Arc::new(RateLimiter::per_minute(0)),
        Arc::new(ApprovalRouter::new()),
    );

    // Authorized round-trip.
    runner.dispatch(event("alice", "hello there")).await;
    settle().await;
    assert!(adapter.texts().contains(&"echo: hello there".to_owned()));

    // Stranger denied; alice issues a code; stranger redeems → authorized.
    runner.dispatch(event("bob", "let me in")).await;
    assert!(adapter.texts().last().unwrap().contains("Not authorized"));
    runner.dispatch(event("alice", "/pair")).await;
    let code = adapter
        .texts()
        .last()
        .unwrap()
        .split_whitespace()
        .last()
        .unwrap()
        .to_owned();
    runner.dispatch(event("bob", &code)).await;
    assert!(adapter.texts().last().unwrap().contains("Paired"));
    runner.dispatch(event("bob", "hi from bob")).await;
    settle().await;
    assert!(adapter.texts().contains(&"echo: hi from bob".to_owned()));
}

#[tokio::test]
async fn stop_bypasses_the_busy_guard_and_cancels_the_turn() {
    let adapter = Arc::new(MockAdapter::default());
    let runner = GatewayRunner::new(
        adapter.clone(),
        Arc::new(SleepyHandler),
        allow(&["alice"]),
        Arc::new(RateLimiter::per_minute(0)),
        Arc::new(ApprovalRouter::new()),
    );

    runner.dispatch(event("alice", "do something slow")).await;
    settle().await;
    // Plain messages are guarded while the turn runs…
    runner.dispatch(event("alice", "are you done?")).await;
    assert!(adapter.texts().last().unwrap().contains("Still working"));
    // …but /stop reaches the runner and cancels the in-flight turn.
    runner.dispatch(event("alice", "/stop")).await;
    settle().await;
    let texts = adapter.texts();
    assert!(texts.iter().any(|t| t.contains("Stopping")));
    assert!(
        texts.iter().any(|t| t.contains("interrupted")),
        "turn must end interrupted"
    );

    // Guard released — next message runs again.
    runner.dispatch(event("alice", "/stop")).await;
    assert!(
        adapter
            .texts()
            .last()
            .unwrap()
            .contains("Nothing is running")
    );
}

#[tokio::test]
async fn a_turn_with_no_final_text_still_answers_the_user() {
    let adapter = Arc::new(MockAdapter::default());
    let runner = GatewayRunner::new(
        adapter.clone(),
        Arc::new(crate::SilentHandler),
        allow(&["alice"]),
        Arc::new(RateLimiter::per_minute(0)),
        Arc::new(ApprovalRouter::new()),
    );

    runner.dispatch(event("alice", "make me a document")).await;
    settle().await;
    // An empty send is rejected by the platform API and swallowed, so the user
    // would otherwise get nothing at all AFTER the work was done.
    let last = adapter.texts().last().cloned().unwrap_or_default();
    assert!(!last.trim().is_empty(), "must never reply with silence");
}

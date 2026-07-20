//! Rerouting policy: stickiness, shared registry health, and which error
//! classes trigger failover.

use crate::{Flaky, request};
use regent_providers::{ChatProvider, FallbackChat, FallbackHealth, ProviderError};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn reroutes_when_primary_down_then_sticks_to_the_survivor() {
    let primary = Flaky::failing_with("primary", || ProviderError::Exhausted {
        attempts: 3,
        last: "503".into(),
    });
    let secondary = Flaky::healthy("secondary");
    let chain = FallbackChat::new(vec![primary.clone(), secondary.clone()]).unwrap();

    // Call 1: primary is down → reroute to the secondary.
    let first = chain.complete(&request()).await.unwrap();
    assert!(first.message.content.unwrap().contains("secondary"));

    // Call 2: STICKY — the survivor answered, so the chain starts THERE and does
    // NOT re-hammer the rate-limited primary (nor pay its retry backoff) every
    // turn. Recovery is handled by the shared cooldown used by registry-built
    // chains, not by retrying a dead primary inside this session.
    chain.complete(&request()).await.unwrap();
    assert_eq!(
        primary.calls(),
        1,
        "dead primary not retried within the session (sticky)"
    );
    assert_eq!(secondary.calls(), 2, "stays on the survivor");
}

#[tokio::test]
async fn shared_health_skips_a_recent_failure_across_sessions_then_recovers() {
    let primary = Flaky::failing_with("primary", || ProviderError::Network("down".into()));
    let secondary = Flaky::healthy("secondary");
    let health = Arc::new(FallbackHealth::new(Duration::from_millis(40)));
    let build = || {
        FallbackChat::with_shared_health(
            vec![primary.clone(), secondary.clone()],
            vec!["provider/primary".into(), "provider/secondary".into()],
            Arc::clone(&health),
        )
        .unwrap()
    };

    build().complete(&request()).await.unwrap();
    assert_eq!(primary.calls(), 1);

    // A fresh session shares the registry health and goes straight to the
    // survivor instead of paying the primary timeout again.
    build().complete(&request()).await.unwrap();
    assert_eq!(primary.calls(), 1);
    assert_eq!(secondary.calls(), 2);

    tokio::time::sleep(Duration::from_millis(50)).await;
    build().complete(&request()).await.unwrap();
    assert_eq!(
        primary.calls(),
        2,
        "cooldown expiry re-probes automatically"
    );
}

#[tokio::test]
async fn rate_limited_primary_completes_on_fallback() {
    // 429 on the primary is transient → fail over and complete on the fallback.
    let primary = Flaky::failing_with("primary", || ProviderError::RateLimited {
        retry_after_ms: None,
    });
    let secondary = Flaky::healthy("secondary");
    let chain = FallbackChat::new(vec![primary.clone(), secondary.clone()]).unwrap();

    let response = chain.complete(&request()).await.unwrap();
    assert!(response.message.content.unwrap().contains("secondary"));
    assert_eq!(primary.calls(), 1, "primary attempted once");
    assert_eq!(secondary.calls(), 1, "fallback served the answer");
}

#[tokio::test]
async fn auth_errors_fail_over_but_client_errors_do_not() {
    let bad_key = Flaky::failing_with("bad-key", || ProviderError::Auth { status: 401 });
    let healthy = Flaky::healthy("backup");
    let chain = FallbackChat::new(vec![bad_key, healthy]).unwrap();
    assert!(chain.complete(&request()).await.is_ok());

    let bad_request = Flaky::failing_with("bad-request", || ProviderError::Api {
        status: 400,
        body: "malformed".into(),
    });
    let never_reached = Flaky::healthy("unreachable");
    let chain = FallbackChat::new(vec![bad_request, never_reached.clone()]).unwrap();
    let error = chain.complete(&request()).await.unwrap_err();
    assert!(matches!(error, ProviderError::Api { status: 400, .. }));
    assert_eq!(never_reached.calls(), 0, "4xx must not trigger failover");
}

#[tokio::test]
async fn whole_chain_down_returns_last_error_and_empty_chain_rejected() {
    let a = Flaky::failing_with("a", || ProviderError::RateLimited {
        retry_after_ms: None,
    });
    let b = Flaky::failing_with("b", || ProviderError::Network("refused".into()));
    let chain = FallbackChat::new(vec![a, b]).unwrap();
    let error = chain.complete(&request()).await.unwrap_err();
    assert!(matches!(error, ProviderError::Network(_)));

    assert!(FallbackChat::new(vec![]).is_err());
}

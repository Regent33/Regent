//! W2 — failover must mitigate an outage, not amplify it.
//!
//! Measured on 2026-07-26: 421 rate-limited responses against ~50 provider
//! selections, clustering 81 in a single minute. Every rate-limit line was a
//! chain hop. Each provider call already retries internally (3 attempts), so an
//! unbounded walk multiplies one turn by `attempts x chain_length`.

use crate::{Flaky, request};
use regent_providers::{ChatProvider, FallbackChat, FallbackHealth, ProviderError};
use std::sync::Arc;
use std::time::Duration;

fn rate_limited() -> ProviderError {
    ProviderError::RateLimited {
        retry_after_ms: None,
    }
}

/// The amplification bound. A long chain of failing providers must not cost one
/// request an attempt on every single member.
#[tokio::test]
async fn one_request_cannot_walk_an_entire_failing_chain() {
    let members: Vec<Arc<Flaky>> = (0..6)
        .map(|_| Flaky::failing_with("dead", rate_limited))
        .collect();
    let chain = FallbackChat::with_shared_health(
        members
            .iter()
            .map(|m| m.clone() as Arc<dyn ChatProvider>)
            .collect(),
        (0..6).map(|i| format!("dead-{i}")).collect(),
        // A long cooldown so this test measures the HOP CAP, not recovery.
        Arc::new(FallbackHealth::new(Duration::from_secs(300))),
    )
    .unwrap();

    assert!(chain.complete(&request()).await.is_err(), "all are down");

    let attempted: u32 = members.iter().map(|m| m.calls()).sum();
    assert_eq!(
        attempted, 3,
        "one request must attempt the first provider plus 2 hops — not all 6"
    );
}

/// The cap's real trade, stated plainly. On a COLD chain it strands the first
/// request — that request never reaches the healthy member. The knowledge is
/// not lost: the attempt records cooldowns, and the NEXT request skips straight
/// past them. A co-review caught the original version of this test claiming the
/// cap "never costs us a reachable provider" while demonstrating the opposite.
#[tokio::test]
async fn the_cap_strands_the_first_request_then_the_next_one_gets_through() {
    let health = Arc::new(FallbackHealth::new(Duration::from_secs(300)));
    let dead: Vec<Arc<Flaky>> = (0..4)
        .map(|_| Flaky::failing_with("dead", rate_limited))
        .collect();
    let healthy = Flaky::healthy("healthy");

    let mut providers: Vec<Arc<dyn ChatProvider>> = dead
        .iter()
        .map(|d| d.clone() as Arc<dyn ChatProvider>)
        .collect();
    providers.push(healthy.clone());
    let keys: Vec<String> = (0..4)
        .map(|i| format!("dead-{i}"))
        .chain(std::iter::once("healthy".to_owned()))
        .collect();

    let build = || {
        FallbackChat::with_shared_health(providers.clone(), keys.clone(), Arc::clone(&health))
            .unwrap()
    };

    // First request: capped, so it does NOT reach the healthy member. This is
    // the cost, asserted rather than glossed.
    let first = build().complete(&request()).await;
    assert!(
        first.is_err(),
        "the cold-chain request is stranded — that is the trade"
    );
    assert_eq!(healthy.calls(), 0, "it never got as far as the healthy one");

    // Next request skips the cooling members entirely and lands on the healthy
    // one. This is why the cap is safe: it bounds work per request, and the
    // cooldown carries the knowledge forward.
    let second = build().complete(&request()).await;
    assert!(
        second.unwrap().message.content.unwrap().contains("healthy"),
        "the next request reaches it, because the cooldowns carried forward"
    );
}

/// A 429 that states its own wait is cooled for THAT long. Previously every
/// failure got a flat 30s, so a provider asking for 90s was re-hit at 30s —
/// one of the ways a single outage became repeated hits.
#[tokio::test(start_paused = true)]
async fn a_rate_limit_is_cooled_for_as_long_as_the_provider_asked() {
    fn asks_for_two_minutes() -> ProviderError {
        ProviderError::RateLimited {
            retry_after_ms: Some(120_000),
        }
    }
    let limited = Flaky::failing_with("limited", asks_for_two_minutes);
    let health = Arc::new(FallbackHealth::new(Duration::from_secs(30)));
    let providers: Vec<Arc<dyn ChatProvider>> = vec![limited.clone(), Flaky::healthy("backup")];
    let keys = vec!["limited".to_owned(), "backup".to_owned()];

    let build = || {
        FallbackChat::with_shared_health(providers.clone(), keys.clone(), Arc::clone(&health))
            .unwrap()
    };
    let _ = build().complete(&request()).await;
    let after_first = limited.calls();

    // Past the flat 30s cooldown, but well inside the 120s the provider asked
    // for: a fresh chain must still skip it.
    tokio::time::advance(Duration::from_secs(45)).await;
    let _ = build().complete(&request()).await;
    assert_eq!(
        limited.calls(),
        after_first,
        "the provider's own retry-after must outrank our flat guess"
    );

    // Past its stated window: it is probed again.
    tokio::time::advance(Duration::from_secs(90)).await;
    let _ = build().complete(&request()).await;
    assert!(
        limited.calls() > after_first,
        "recovery must still be automatic once the stated window passes"
    );
}

/// A locally CONFIGURED cooldown is the operator's call and is not capped —
/// only a server-stated `retry-after` is. Capping both silently overrode an
/// operator asking for a longer window (co-review finding).
#[tokio::test(start_paused = true)]
async fn a_configured_cooldown_longer_than_the_cap_is_respected() {
    let down = Flaky::failing_with("down", rate_limited); // no retry-after
    let health = Arc::new(FallbackHealth::new(Duration::from_secs(20 * 60)));
    let providers: Vec<Arc<dyn ChatProvider>> = vec![down.clone(), Flaky::healthy("backup")];
    let keys = vec!["down".to_owned(), "backup".to_owned()];
    let build = || {
        FallbackChat::with_shared_health(providers.clone(), keys.clone(), Arc::clone(&health))
            .unwrap()
    };

    let _ = build().complete(&request()).await;
    let after_first = down.calls();

    // Well past the 5-minute cap on STATED waits, but inside the configured 20.
    tokio::time::advance(Duration::from_secs(10 * 60)).await;
    let _ = build().complete(&request()).await;
    assert_eq!(
        down.calls(),
        after_first,
        "the configured cooldown is not truncated by the stated-wait cap"
    );
}

/// A hostile or mistaken header cannot park a provider indefinitely.
#[tokio::test(start_paused = true)]
async fn an_absurd_retry_after_is_capped() {
    fn asks_for_a_day() -> ProviderError {
        ProviderError::RateLimited {
            retry_after_ms: Some(24 * 60 * 60 * 1000),
        }
    }
    let rude = Flaky::failing_with("rude", asks_for_a_day);
    let health = Arc::new(FallbackHealth::new(Duration::from_secs(30)));
    let providers: Vec<Arc<dyn ChatProvider>> = vec![rude.clone(), Flaky::healthy("backup")];
    let keys = vec!["rude".to_owned(), "backup".to_owned()];
    let build = || {
        FallbackChat::with_shared_health(providers.clone(), keys.clone(), Arc::clone(&health))
            .unwrap()
    };

    let _ = build().complete(&request()).await;
    let after_first = rude.calls();

    tokio::time::advance(Duration::from_secs(6 * 60)).await;
    let _ = build().complete(&request()).await;
    assert!(
        rude.calls() > after_first,
        "the cooldown is capped at 5 minutes regardless of the header"
    );
}

/// When everything is cooling the chain still makes ONE best-effort attempt —
/// on the member closest to recovering, rather than whichever happened to sit
/// last in the chain.
#[tokio::test(start_paused = true)]
async fn an_all_cooling_chain_probes_the_one_closest_to_recovery() {
    fn short_wait() -> ProviderError {
        ProviderError::RateLimited {
            retry_after_ms: Some(60_000),
        }
    }
    fn long_wait() -> ProviderError {
        ProviderError::RateLimited {
            retry_after_ms: Some(240_000),
        }
    }
    let soon = Flaky::failing_with("soon", short_wait);
    let later = Flaky::failing_with("later", long_wait);
    let health = Arc::new(FallbackHealth::new(Duration::from_secs(30)));
    let providers: Vec<Arc<dyn ChatProvider>> = vec![soon.clone(), later.clone()];
    let keys = vec!["soon".to_owned(), "later".to_owned()];
    let build = || {
        FallbackChat::with_shared_health(providers.clone(), keys.clone(), Arc::clone(&health))
            .unwrap()
    };

    // Both fail and start cooling, with different stated windows.
    let _ = build().complete(&request()).await;
    let (soon_before, later_before) = (soon.calls(), later.calls());

    // Everything is cooling. The single probe goes to `soon` — the one whose
    // window ends first — not to the tail of the chain.
    tokio::time::advance(Duration::from_secs(5)).await;
    let _ = build().complete(&request()).await;
    assert_eq!(
        soon.calls(),
        soon_before + 1,
        "the probe goes to the member closest to recovering"
    );
    assert_eq!(later.calls(), later_before, "not to the tail of the chain");
}

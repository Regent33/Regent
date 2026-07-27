//! Retry-loop tests: the per-error attempt budget (W2) and `retry-after`
//! parsing. Split from `http.rs` for the file-size rule.

use super::*;
use crate::domain::entities::ChatResponse;
use std::sync::atomic::{AtomicU32, Ordering};
/// Gap P-retry: a 429 carrying `retry-after` waits at least that long
/// (base backoff here is 1ms, so the wait is attributable to the header).
#[tokio::test]
async fn retry_after_header_drives_the_delay() {
    let policy = RetryPolicy {
        max_attempts: 3,
        base_delay_ms: 1,
        max_delay_ms: 60_000,
        jitter: false,
    };
    let calls = AtomicU32::new(0);
    let started = std::time::Instant::now();
    let result = run_with_retry(&policy, || {
        let n = calls.fetch_add(1, Ordering::SeqCst);
        async move {
            if n == 0 {
                Err(ProviderError::RateLimited {
                    retry_after_ms: Some(300),
                })
            } else {
                Ok(ChatResponse {
                    message: regent_kernel::ChatMessage::assistant(Some("ok".to_owned()), vec![]),
                    usage: or_core::TokenUsage::default(),
                    finish_reason: None,
                })
            }
        }
    })
    .await;
    assert!(result.is_ok());
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let waited = started.elapsed().as_millis();
    assert!(waited >= 300, "waited only {waited}ms");
}

fn policy() -> RetryPolicy {
    RetryPolicy {
        max_attempts: 3,
        base_delay_ms: 1,
        max_delay_ms: 60_000,
        jitter: false,
    }
}

async fn attempts_against(error: fn() -> ProviderError) -> u32 {
    let calls = AtomicU32::new(0);
    let _ = run_with_retry(&policy(), || {
        calls.fetch_add(1, Ordering::SeqCst);
        async move { Err(error()) }
    })
    .await;
    calls.load(Ordering::SeqCst)
}

/// W2, the amplification itself. A 429 with no `retry-after` gets NO
/// in-place retry: any backoff we pick is a guess, and guessing is what
/// turned one bad day into 421 rate-limited responses across ~50 provider
/// selections. The chain has other providers with independent quota — hand
/// off instead.
#[tokio::test]
async fn a_blind_rate_limit_is_not_retried_in_place() {
    assert_eq!(
        attempts_against(|| ProviderError::RateLimited {
            retry_after_ms: None
        })
        .await,
        1,
        "one call, then fail over — not three"
    );
}

/// A 429 that stated its wait gets one retry: the provider told us exactly
/// when it would serve again, which beats migrating the conversation off it
/// for a momentary burst.
#[tokio::test]
async fn a_stated_rate_limit_gets_one_in_place_retry() {
    assert_eq!(
        attempts_against(|| ProviderError::RateLimited {
            retry_after_ms: Some(1)
        })
        .await,
        2
    );
}

/// Everything else keeps the full budget — a 5xx or a dropped connection IS
/// the transient blip retries were designed for.
#[tokio::test]
async fn other_transient_failures_keep_the_full_budget() {
    assert_eq!(
        attempts_against(|| ProviderError::Network("connection reset".into())).await,
        3
    );
    assert_eq!(
        attempts_against(|| ProviderError::Api {
            status: 503,
            body: "upstream down".into()
        })
        .await,
        3
    );
}

#[test]
fn retry_after_parses_numeric_seconds_only() {
    let mut headers = reqwest::header::HeaderMap::new();
    assert_eq!(retry_after_ms(&headers), None);
    headers.insert("retry-after", "12".parse().unwrap());
    assert_eq!(retry_after_ms(&headers), Some(12_000));
    headers.insert(
        "retry-after",
        "Wed, 21 Oct 2026 07:28:00 GMT".parse().unwrap(),
    );
    assert_eq!(retry_after_ms(&headers), None);
}

/// A connect failure (deterministic + fast: a bound-then-dropped local port
/// refuses the connection) maps to the stable connection text — proving the
/// classifier keys on `is_connect`, not a reqwest string.
#[tokio::test]
async fn connect_failures_map_to_stable_connection_text() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener); // free the port so the connect is refused
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let err = client
        .get(format!("http://{addr}/"))
        .send()
        .await
        .expect_err("connect must fail");
    assert!(err.is_connect(), "precondition: {err}");
    match network_error(&err) {
        ProviderError::Network(msg) => assert_eq!(msg, "could not connect to the provider"),
        other => panic!("expected Network, got {other:?}"),
    }
}

/// A total-request timeout (connection accepted but never answered) maps to
/// the stable timeout text — the case Nemotron's slow first token hit.
#[tokio::test]
async fn request_timeouts_map_to_stable_timeout_text() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    // Accept and hold the socket open without ever replying; the runtime
    // cancels this task when the test returns (no unbounded wait).
    tokio::spawn(async move {
        let _held = listener.accept().await;
        tokio::time::sleep(Duration::from_secs(5)).await;
    });
    let err = reqwest::Client::new()
        .get(format!("http://{addr}/"))
        .timeout(Duration::from_millis(200))
        .send()
        .await
        .expect_err("request must time out");
    assert!(err.is_timeout(), "precondition: {err}");
    match network_error(&err) {
        ProviderError::Network(msg) => assert_eq!(msg, "request timed out"),
        other => panic!("expected Network, got {other:?}"),
    }
}

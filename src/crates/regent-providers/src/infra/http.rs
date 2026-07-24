//! Shared HTTP plumbing for the provider adapters: the retry loop (identical
//! across providers) and a UTF-8-safe error-body truncator.

use crate::domain::entities::ChatResponse;
use crate::domain::errors::ProviderError;
use or_core::{BackoffStrategy, RetryPolicy};
use std::future::Future;
use std::time::Duration;

/// Runs `attempt` under the retry policy: retryable errors back off with
/// exponential full jitter; the first success returns; exhaustion surfaces as
/// `Exhausted`. Shared by every non-streaming provider call.
pub(crate) async fn run_with_retry<F, Fut>(
    retry: &RetryPolicy,
    mut attempt: F,
) -> Result<ChatResponse, ProviderError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<ChatResponse, ProviderError>>,
{
    let mut last_error: Option<ProviderError> = None;
    for n in 1..=retry.max_attempts {
        match attempt().await {
            Ok(response) => return Ok(response),
            Err(error) if error.is_retryable() && n < retry.max_attempts => {
                // A server-stated `retry-after` beats our jittered guess; still
                // capped so a hostile header can't stall the loop for minutes.
                let delay = error.retry_after_ms().map_or_else(
                    || BackoffStrategy::ExponentialFullJitter.delay_ms(retry, n),
                    |after| after.min(retry.max_delay_ms),
                );
                tracing::warn!(attempt = n, delay_ms = delay, %error, "provider call retrying");
                tokio::time::sleep(Duration::from_millis(delay)).await;
                last_error = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(ProviderError::Exhausted {
        attempts: retry.max_attempts,
        last: last_error.map_or_else(|| "unknown".into(), |e| e.to_string()),
    })
}

/// Parses a `retry-after` header into milliseconds.
// ponytail: numeric-seconds form only; the HTTP-date form is rare on LLM APIs —
// parse it here if a provider ever sends one.
pub(crate) fn retry_after_ms(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get("retry-after")?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(|secs| secs.saturating_mul(1000))
}

/// Truncates `text` to at most `max` bytes on a char boundary, appending `…`.
pub(crate) fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        text.to_owned()
    } else {
        let mut end = max;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &text[..end])
    }
}

/// Maps a reqwest transport error to `ProviderError::Network` with stable,
/// classifiable text so the deacon can humanize it without pattern-matching
/// reqwest's version/OS-specific strings. Connect-phase failures are checked
/// FIRST: a refused *or* timed-out connection is a reachability problem (a
/// connect timeout reports both `is_connect` and `is_timeout`), and "check the
/// URL / your connection" is the right advice. A timeout with the connection
/// already established (`is_connect` false) is the slow-first-token case — the
/// total per-request `.timeout(...)` elapsing — and maps to the retry advice.
/// Every other transport error keeps its own message.
pub(crate) fn network_error(e: &reqwest::Error) -> ProviderError {
    if e.is_connect() {
        ProviderError::Network("could not connect to the provider".into())
    } else if e.is_timeout() {
        ProviderError::Network("request timed out".into())
    } else {
        ProviderError::Network(e.to_string())
    }
}

#[cfg(test)]
mod tests {
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
                        message: regent_kernel::ChatMessage::assistant(
                            Some("ok".to_owned()),
                            vec![],
                        ),
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
}

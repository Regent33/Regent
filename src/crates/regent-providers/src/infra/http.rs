//! Shared HTTP plumbing for the provider adapters: the retry loop (identical
//! across providers) and a UTF-8-safe error-body truncator.

use crate::domain::entities::ChatResponse;
use crate::domain::errors::ProviderError;
use or_core::{BackoffStrategy, RetryPolicy};
use std::future::Future;
use std::time::Duration;

/// How long ONE completion request may take, end to end.
///
/// This bounds the whole request — headers *and* body — so for a completion it
/// is a cap on total generation time, not just on reaching the endpoint. At the
/// old 120s a large model doing real work died mid-turn: a 550B model handed a
/// fetched Wikipedia article timed out at exactly 120s with "network error:
/// request timed out", while the same session's neighbouring turns legitimately
/// ran 185s and 199s. The chat simply stopped, and the user re-sent by hand.
///
/// A dead endpoint is still caught in 10s by the connect timeout, which is the
/// check that actually distinguishes "down" from "thinking". This one only has
/// to be longer than the slowest honest completion.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(600);

/// How many attempts a RATE LIMIT gets in place, when the provider told us
/// nothing about when to come back.
///
/// W2 — this is the amplification. A 429 is not a transient blip: the provider
/// has explicitly said stop. Retrying it in place on a guessed backoff, three
/// times, for every provider in the chain, is how one bad day produced 421
/// rate-limited responses against ~50 provider selections. With no `retry-after`
/// to go on, any wait we pick is a guess, and the chain has other providers with
/// independent quota — so hand off immediately instead of guessing.
const BLIND_RATE_LIMIT_ATTEMPTS: u32 = 1;

/// A rate limit that DID state its wait gets one in-place retry: the provider
/// told us exactly when it would serve again, which is worth honouring for a
/// momentary burst rather than migrating the whole conversation off it.
const STATED_RATE_LIMIT_ATTEMPTS: u32 = 2;

/// The attempt budget for `error`, which is not always the policy's: see the
/// two constants above.
fn budget_for(retry: &RetryPolicy, error: &ProviderError) -> u32 {
    match error {
        ProviderError::RateLimited { retry_after_ms } => {
            retry.max_attempts.min(if retry_after_ms.is_some() {
                STATED_RATE_LIMIT_ATTEMPTS
            } else {
                BLIND_RATE_LIMIT_ATTEMPTS
            })
        }
        _ => retry.max_attempts,
    }
}

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
            Err(error) if error.is_retryable() && n < budget_for(retry, &error) => {
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
#[path = "tests/http.rs"]
mod tests;

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
                let delay = BackoffStrategy::ExponentialFullJitter.delay_ms(retry, n);
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

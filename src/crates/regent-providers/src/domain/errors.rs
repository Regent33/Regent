use regent_kernel::RegentError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("network error: {0}")]
    Network(String),

    #[error("authentication failed (HTTP {status})")]
    Auth { status: u16 },

    #[error("API error (HTTP {status}): {body}")]
    Api { status: u16, body: String },

    #[error("rate limited (HTTP 429)")]
    RateLimited {
        /// Server-requested wait from the `retry-after` header, when present.
        retry_after_ms: Option<u64>,
    },

    #[error("response parse error: {0}")]
    Parse(String),

    #[error("retries exhausted after {attempts} attempts: {last}")]
    Exhausted { attempts: u32, last: String },
}

impl ProviderError {
    /// Transient failures worth retrying (fallback semantics:
    /// 429 + 5xx + transport errors retry; 4xx auth/client errors do not).
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Network(_) | Self::RateLimited { .. })
            || matches!(self, Self::Api { status, .. } if *status >= 500)
    }

    /// The server-requested retry delay, when the 429 carried one (gap P-retry:
    /// honoring it beats guessing with jitter).
    #[must_use]
    pub fn retry_after_ms(&self) -> Option<u64> {
        match self {
            Self::RateLimited { retry_after_ms } => *retry_after_ms,
            _ => None,
        }
    }
}

impl From<ProviderError> for RegentError {
    fn from(value: ProviderError) -> Self {
        RegentError::Provider(value.to_string())
    }
}

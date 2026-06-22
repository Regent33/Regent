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
    RateLimited,

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
        matches!(self, Self::Network(_) | Self::RateLimited)
            || matches!(self, Self::Api { status, .. } if *status >= 500)
    }
}

impl From<ProviderError> for RegentError {
    fn from(value: ProviderError) -> Self {
        RegentError::Provider(value.to_string())
    }
}

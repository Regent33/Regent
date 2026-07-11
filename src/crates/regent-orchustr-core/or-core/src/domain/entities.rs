use rand::RngExt;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    /// SPL P2 (`docs/proposal/token-efficiency-architecture-v1.md` §3.3): input
    /// tokens served from the provider's prompt cache this call (billed ~0.1×).
    /// `None` when the provider doesn't report cache usage — additive, so older
    /// call sites and non-caching providers are unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<u32>,
    /// SPL P2: input tokens written to the cache this call (the one-time
    /// ~1.25×/2× seed). `None` when unreported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenBudget {
    pub max_context_tokens: u32,
    pub max_completion_tokens: u32,
}

impl TokenBudget {
    #[must_use]
    pub fn fits(&self, prompt_tokens: u32, completion_tokens: u32) -> bool {
        prompt_tokens.saturating_add(completion_tokens) <= self.max_context_tokens
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub jitter: bool,
}

impl RetryPolicy {
    #[must_use]
    pub fn no_retry() -> Self {
        Self {
            max_attempts: 1,
            base_delay_ms: 0,
            max_delay_ms: 0,
            jitter: false,
        }
    }

    #[must_use]
    pub fn default_llm() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 500,
            max_delay_ms: 30_000,
            jitter: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BackoffStrategy {
    Exponential,
    ExponentialFullJitter,
    Fixed,
}

impl BackoffStrategy {
    #[must_use]
    pub fn delay_ms(&self, policy: &RetryPolicy, attempt: u32) -> u64 {
        let capped_attempt = attempt.saturating_sub(1).min(31);
        let multiplier = 2u64.saturating_pow(capped_attempt);
        let base = policy
            .base_delay_ms
            .saturating_mul(multiplier)
            .min(policy.max_delay_ms);
        match self {
            Self::Fixed => policy.base_delay_ms.min(policy.max_delay_ms),
            Self::Exponential => base,
            Self::ExponentialFullJitter => {
                if !policy.jitter || base == 0 {
                    base
                } else {
                    let mut rng = rand::rng();
                    rng.random_range(0..=base)
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VectorRecord {
    pub id: String,
    pub score: f32,
    pub metadata: serde_json::Value,
}

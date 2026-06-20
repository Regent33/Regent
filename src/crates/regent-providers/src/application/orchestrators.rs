//! Provider failover chain (the Hermes `fallback_providers` semantics):
//! when the active provider fails with rate-limit / 5xx / network / auth /
//! retry-exhaustion, move forward through the chain and **stay** on the
//! survivor (sticky) so the rest of the conversation uses one provider —
//! flapping back and forth would thrash the provider-side prompt cache.

use crate::domain::contracts::{ChatProvider, DeltaSink};
use crate::domain::entities::{ChatRequest, ChatResponse};
use crate::domain::errors::ProviderError;
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub struct FallbackChat {
    providers: Vec<Arc<dyn ChatProvider>>,
    active: AtomicUsize,
}

impl FallbackChat {
    /// `providers` is ordered: primary first. Must be non-empty.
    pub fn new(providers: Vec<Arc<dyn ChatProvider>>) -> Result<Self, ProviderError> {
        if providers.is_empty() {
            return Err(ProviderError::Parse("fallback chain cannot be empty".into()));
        }
        Ok(Self { providers, active: AtomicUsize::new(0) })
    }

    #[must_use]
    pub fn active_index(&self) -> usize {
        self.active.load(Ordering::Relaxed)
    }
}

/// Failover-worthy: everything transient plus auth (a dead key on provider A
/// says nothing about provider B). Non-retryable 4xx (bad request, parse)
/// would fail identically everywhere — surface immediately instead.
fn should_failover(error: &ProviderError) -> bool {
    error.is_retryable()
        || matches!(error, ProviderError::Auth { .. } | ProviderError::Exhausted { .. })
}

#[async_trait]
impl ChatProvider for FallbackChat {
    async fn complete(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        let start = self.active.load(Ordering::Relaxed);
        let mut last_error: Option<ProviderError> = None;
        // Forward-only: never fall back to a provider that already failed.
        for index in start..self.providers.len() {
            match self.providers[index].complete(request).await {
                Ok(response) => {
                    if index != start {
                        tracing::warn!(
                            from = self.providers[start].model(),
                            to = self.providers[index].model(),
                            "provider failover engaged (sticky)"
                        );
                    }
                    self.active.store(index, Ordering::Relaxed);
                    return Ok(response);
                }
                Err(error) if should_failover(&error) && index + 1 < self.providers.len() => {
                    tracing::warn!(provider = self.providers[index].model(), %error,
                                   "provider failed; trying next in chain");
                    last_error = Some(error);
                }
                Err(error) => return Err(error),
            }
        }
        Err(last_error.unwrap_or_else(|| ProviderError::Parse("empty fallback chain".into())))
    }

    /// Streaming failover. A provider is only abandoned if it fails **before
    /// emitting any delta** — once text has reached the user, re-running on
    /// another provider would duplicate it, so a mid-stream failure surfaces.
    async fn complete_streaming(
        &self,
        request: &ChatRequest,
        on_delta: DeltaSink<'_>,
    ) -> Result<ChatResponse, ProviderError> {
        let start = self.active.load(Ordering::Relaxed);
        let mut last_error: Option<ProviderError> = None;
        for index in start..self.providers.len() {
            let emitted = AtomicBool::new(false);
            let wrapped = |fragment: &str| {
                emitted.store(true, Ordering::Relaxed);
                on_delta(fragment);
            };
            match self.providers[index].complete_streaming(request, &wrapped).await {
                Ok(response) => {
                    if index != start {
                        tracing::warn!(
                            from = self.providers[start].model(),
                            to = self.providers[index].model(),
                            "provider failover engaged (sticky)"
                        );
                    }
                    self.active.store(index, Ordering::Relaxed);
                    return Ok(response);
                }
                Err(error)
                    if should_failover(&error)
                        && !emitted.load(Ordering::Relaxed)
                        && index + 1 < self.providers.len() =>
                {
                    tracing::warn!(provider = self.providers[index].model(), %error,
                                   "provider failed pre-stream; trying next in chain");
                    last_error = Some(error);
                }
                Err(error) => return Err(error),
            }
        }
        Err(last_error.unwrap_or_else(|| ProviderError::Parse("empty fallback chain".into())))
    }

    fn model(&self) -> &str {
        self.providers[self.active_index()].model()
    }
}

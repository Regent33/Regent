//! Provider failover chain (the `fallback_providers` semantics):
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

/// Fired when the answering provider changes: `(on_fallback, model_id)`.
/// `on_fallback` is true whenever a non-primary (index > 0) provider answered,
/// so a UI can show the real model in play during a failover, and clear it on
/// recovery. Kept generic (no deacon types) so this crate stays standalone.
pub type ActiveChangeFn = Arc<dyn Fn(bool, &str) + Send + Sync>;

pub struct FallbackChat {
    providers: Vec<Arc<dyn ChatProvider>>,
    active: AtomicUsize,
    notified: AtomicUsize,
    on_change: Option<ActiveChangeFn>,
}

impl FallbackChat {
    /// `providers` is ordered: primary first. Must be non-empty.
    pub fn new(providers: Vec<Arc<dyn ChatProvider>>) -> Result<Self, ProviderError> {
        if providers.is_empty() {
            return Err(ProviderError::Parse(
                "fallback chain cannot be empty".into(),
            ));
        }
        Ok(Self {
            providers,
            active: AtomicUsize::new(0),
            notified: AtomicUsize::new(0),
            on_change: None,
        })
    }

    /// Attach a callback fired whenever the answering provider changes (failover
    /// engaged or recovered) — for surfacing the live model to the UI.
    #[must_use]
    pub fn with_on_change(mut self, cb: ActiveChangeFn) -> Self {
        self.on_change = Some(cb);
        self
    }

    #[must_use]
    pub fn active_index(&self) -> usize {
        self.active.load(Ordering::Relaxed)
    }

    /// Record which provider answered and, if it changed since the last
    /// notification, fire the on-change callback (index 0 = primary/recovered).
    fn record(&self, index: usize) {
        self.active.store(index, Ordering::Relaxed);
        if self.notified.swap(index, Ordering::Relaxed) != index
            && let Some(cb) = &self.on_change
        {
            cb(index != 0, self.providers[index].model());
        }
    }
}

/// Failover-worthy: everything transient plus auth (a dead key on provider A
/// says nothing about provider B). Non-retryable 4xx (bad request, parse)
/// would fail identically everywhere — surface immediately instead.
fn should_failover(error: &ProviderError) -> bool {
    error.is_retryable()
        || matches!(
            error,
            ProviderError::Auth { .. } | ProviderError::Exhausted { .. }
        )
}

#[async_trait]
impl ChatProvider for FallbackChat {
    async fn complete(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        // Primary-first (recovering): every call starts at the primary, so the
        // chain reroutes when it's unavailable AND returns to it the moment it
        // recovers — no manual reset. `active` just records the last provider
        // that answered (for `model()` display), not a sticky start point.
        let start = 0;
        let mut last_error: Option<ProviderError> = None;
        for index in start..self.providers.len() {
            match self.providers[index].complete(request).await {
                Ok(response) => {
                    if index != start {
                        tracing::warn!(
                            from = self.providers[start].model(),
                            to = self.providers[index].model(),
                            "provider failover engaged (recovering)"
                        );
                    }
                    self.record(index);
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
        let start = 0; // primary-first (recovering) — see `complete`.
        let mut last_error: Option<ProviderError> = None;
        for index in start..self.providers.len() {
            let emitted = AtomicBool::new(false);
            let wrapped = |fragment: &str| {
                emitted.store(true, Ordering::Relaxed);
                on_delta(fragment);
            };
            match self.providers[index]
                .complete_streaming(request, &wrapped)
                .await
            {
                Ok(response) => {
                    if index != start {
                        tracing::warn!(
                            from = self.providers[start].model(),
                            to = self.providers[index].model(),
                            "provider failover engaged (recovering)"
                        );
                    }
                    self.record(index);
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

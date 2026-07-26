//! Provider failover chain (the `fallback_providers` semantics):
//! when the active provider fails with rate-limit / 5xx / network / auth /
//! retry-exhaustion, move forward through the chain and **stay** on the
//! survivor (sticky) so the rest of the conversation uses one provider —
//! flapping back and forth would thrash the provider-side prompt cache.

use crate::domain::contracts::{ChatProvider, DeltaSink};
use crate::domain::entities::{ChatRequest, ChatResponse};
use crate::domain::errors::ProviderError;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Fired when the answering provider changes: `(on_fallback, model_id)`.
/// `on_fallback` is true whenever a non-primary (index > 0) provider answered,
/// so a UI can show the real model in play during a failover, and clear it on
/// recovery. Kept generic (no deacon types) so this crate stays standalone.
pub type ActiveChangeFn = Arc<dyn Fn(bool, &str) + Send + Sync>;

/// Cross-chain provider health. A registry shares one instance across all
/// sessions, so opening a new chat does not immediately re-pay the timeout of
/// a provider that just failed. Cooldown expiry provides automatic recovery.
pub struct FallbackHealth {
    cooldown: Duration,
    cooling_until: Mutex<HashMap<String, Instant>>,
}

impl Default for FallbackHealth {
    fn default() -> Self {
        Self::new(Duration::from_secs(30))
    }
}

impl FallbackHealth {
    #[must_use]
    pub fn new(cooldown: Duration) -> Self {
        Self {
            cooldown,
            cooling_until: Mutex::new(HashMap::new()),
        }
    }

    fn cooling(&self, key: &str) -> bool {
        self.cooling_until
            .lock()
            .unwrap()
            .get(key)
            .is_some_and(|until| *until > Instant::now())
    }

    fn failed(&self, key: &str) {
        self.cooling_until
            .lock()
            .unwrap()
            .insert(key.to_owned(), Instant::now() + self.cooldown);
    }

    fn healthy(&self, key: &str) {
        self.cooling_until.lock().unwrap().remove(key);
    }
}

pub struct FallbackChat {
    providers: Vec<Arc<dyn ChatProvider>>,
    health_keys: Vec<String>,
    health: Arc<FallbackHealth>,
    active: AtomicUsize,
    notified: AtomicUsize,
    on_change: Option<ActiveChangeFn>,
}

impl FallbackChat {
    /// `providers` is ordered: primary first. Must be non-empty.
    pub fn new(providers: Vec<Arc<dyn ChatProvider>>) -> Result<Self, ProviderError> {
        let keys = providers.iter().map(|p| p.model().to_owned()).collect();
        Self::with_shared_health(providers, keys, Arc::new(FallbackHealth::default()))
    }

    /// Builds a chain over registry-owned health shared by all sessions.
    pub fn with_shared_health(
        providers: Vec<Arc<dyn ChatProvider>>,
        health_keys: Vec<String>,
        health: Arc<FallbackHealth>,
    ) -> Result<Self, ProviderError> {
        if providers.is_empty() {
            return Err(ProviderError::Parse(
                "fallback chain cannot be empty".into(),
            ));
        }
        if providers.len() != health_keys.len() {
            return Err(ProviderError::Parse(
                "fallback health keys must match provider count".into(),
            ));
        }
        Ok(Self {
            providers,
            health_keys,
            health,
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

    fn candidates(&self, start: usize) -> Vec<usize> {
        let all: Vec<usize> = (start..self.providers.len()).collect();
        let ready: Vec<usize> = all
            .iter()
            .copied()
            .filter(|index| !self.health.cooling(&self.health_keys[*index]))
            .collect();
        if ready.is_empty() {
            // Every link is cooling: make one best-effort attempt instead of
            // serially paying every known-bad provider's timeout again.
            all.last().copied().into_iter().collect()
        } else {
            ready
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
        // Sticky: start from the provider that last answered on THIS chain, so
        // once a turn reroutes to a healthy provider the rest of the SESSION
        // stays on it — no re-hammering a rate-limited primary every turn, and
        // no flapping that thrashes the provider-side prompt cache. Recovery is
        // still automatic and free: a NEW session builds a fresh chain (active=0)
        // that re-probes the primary. (Reverts dd79c4b, whose primary-first
        // "recovering" re-tried the dead primary — and its 429 backoff — on every
        // turn, so a rate-limited primary slowed every turn in the session.)
        let start = self.active.load(Ordering::Relaxed);
        let mut last_error: Option<ProviderError> = None;
        let candidates = self.candidates(start);
        for (position, index) in candidates.iter().copied().enumerate() {
            let has_next = position + 1 < candidates.len();
            match self.providers[index].complete(request).await {
                // A 200 with NOTHING in it — no text, no tool calls, no
                // reasoning — is a provider failing while claiming success, so
                // the chain acts on it. `produced_nothing`, not `is_empty`: a
                // reasoning model that thought and stopped short is alive and
                // the agent repairs it in place; rerouting on that switched a
                // healthy model out from under the user (see entities.rs).
                Ok(response) if response.produced_nothing() && has_next => {
                    let provider = self.providers[index].model().to_owned();
                    self.health.failed(&self.health_keys[index]);
                    tracing::warn!(%provider, "provider returned an empty response; trying next in chain");
                    last_error = Some(ProviderError::Empty { provider });
                }
                Ok(response) => {
                    if response.produced_nothing() {
                        self.health.failed(&self.health_keys[index]);
                    } else {
                        self.health.healthy(&self.health_keys[index]);
                    }
                    if index != start {
                        tracing::warn!(
                            from = self.providers[start].model(),
                            to = self.providers[index].model(),
                            "provider failover engaged (sticky)"
                        );
                    }
                    self.record(index);
                    return Ok(response);
                }
                Err(error) if should_failover(&error) => {
                    self.health.failed(&self.health_keys[index]);
                    if has_next {
                        tracing::warn!(provider = self.providers[index].model(), %error,
                                       "provider failed; trying next in chain");
                        last_error = Some(error);
                    } else {
                        return Err(error);
                    }
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
        let start = self.active.load(Ordering::Relaxed); // sticky — see `complete`.
        let mut last_error: Option<ProviderError> = None;
        let candidates = self.candidates(start);
        for (position, index) in candidates.iter().copied().enumerate() {
            let has_next = position + 1 < candidates.len();
            let emitted = AtomicBool::new(false);
            let wrapped = |fragment: &str| {
                emitted.store(true, Ordering::Relaxed);
                on_delta(fragment);
            };
            match self.providers[index]
                .complete_streaming(request, &wrapped)
                .await
            {
                // Empty 200 with nothing streamed: fail over (same as `complete`).
                // The `!emitted` guard is what makes it safe — a provider that
                // already streamed text can't be re-run without duplicating it,
                // so only a truly silent empty answer reroutes.
                Ok(response)
                    if response.produced_nothing()
                        && !emitted.load(Ordering::Relaxed)
                        && has_next =>
                {
                    let provider = self.providers[index].model().to_owned();
                    self.health.failed(&self.health_keys[index]);
                    tracing::warn!(%provider, "provider streamed an empty response; trying next in chain");
                    last_error = Some(ProviderError::Empty { provider });
                }
                Ok(response) => {
                    if response.produced_nothing() && !emitted.load(Ordering::Relaxed) {
                        self.health.failed(&self.health_keys[index]);
                    } else {
                        self.health.healthy(&self.health_keys[index]);
                    }
                    if index != start {
                        tracing::warn!(
                            from = self.providers[start].model(),
                            to = self.providers[index].model(),
                            "provider failover engaged (sticky)"
                        );
                    }
                    self.record(index);
                    return Ok(response);
                }
                Err(error) if should_failover(&error) && !emitted.load(Ordering::Relaxed) => {
                    self.health.failed(&self.health_keys[index]);
                    if has_next {
                        tracing::warn!(provider = self.providers[index].model(), %error,
                                       "provider failed pre-stream; trying next in chain");
                        last_error = Some(error);
                    } else {
                        return Err(error);
                    }
                }
                Err(error) => return Err(error),
            }
        }
        Err(last_error.unwrap_or_else(|| ProviderError::Parse("empty fallback chain".into())))
    }

    fn model(&self) -> &str {
        self.providers[self.active_index()].model()
    }

    /// Delegate to the ACTIVE chain member — the trait default would consult
    /// only the static table and miss a member's discovered/override window.
    fn context_window(&self) -> Option<u32> {
        self.providers[self.active_index()].context_window()
    }
}

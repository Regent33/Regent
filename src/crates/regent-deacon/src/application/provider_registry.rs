//! Provider registry: resolves a [`ModelRef`] (a configured provider name + a
//! model id) to a `ChatProvider`, memoizing so each `(provider, model)` pair
//! builds once. Reuses [`make_provider_factory`] for the actual wire adapter
//! and [`FallbackChat`] for per-agent fallback chains — no new provider code.
//!
//! This lives in the deacon (not `regent-providers`) because the provider
//! *kinds* and the factory live here; moving them would churn working code for
//! no gain. `ModelRef` is the only shared piece, and it lives in the kernel.

use crate::application::provider_factory::make_provider_factory;
use crate::domain::config::ProviderSpec;
use regent_kernel::ModelRef;
use regent_providers::{ChatProvider, FallbackChat};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("unknown provider '{0}' (not in config.providers)")]
    UnknownProvider(String),
    #[error("provider '{provider}' has no API key (set ${env})")]
    MissingKey { provider: String, env: String },
    #[error("fallback chain is empty")]
    EmptyChain,
}

/// Resolves `ModelRef`s to providers from the configured `providers` map.
/// Cheap to share (`Arc`); the build cache is internally synchronized.
pub struct ProviderRegistry {
    specs: HashMap<String, ProviderSpec>,
    cache: Mutex<HashMap<ModelRef, Arc<dyn ChatProvider>>>,
}

impl ProviderRegistry {
    #[must_use]
    pub fn from_config(specs: &HashMap<String, ProviderSpec>) -> Self {
        Self {
            specs: specs.clone(),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// `true` when at least one provider is configured (so callers can skip the
    /// registry path entirely under today's single-provider setup).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }

    /// Resolve (and memoize) a provider for a model. Typed errors, never panics.
    /// The key is read from the environment at resolve time (never stored).
    /// A ref pinned to a `key_slot` reads that exact slot's var (`<BASE>_<N>`,
    /// slot 1 = the base var) — no fall-through: an explicit slot means THAT
    /// key, and an unset one is a `MissingKey` so a chain skips the link.
    pub fn provider_for(&self, m: &ModelRef) -> Result<Arc<dyn ChatProvider>, RegistryError> {
        if let Some(hit) = self.cache.lock().unwrap().get(m) {
            return Ok(Arc::clone(hit));
        }
        let spec = self
            .specs
            .get(&m.provider)
            .ok_or_else(|| RegistryError::UnknownProvider(m.provider.clone()))?;
        // A provider with no `api_key_env` (e.g. local Ollama on localhost) is
        // KEYLESS: resolve to an empty key and skip the MissingKey gate. A
        // provider that DOES name a key var but has it unset is still MissingKey.
        let key = if spec.api_key_env.is_empty() {
            String::new()
        } else {
            let env_name = match m.key_slot {
                Some(n) if n >= 2 => format!("{}_{n}", spec.api_key_env),
                _ => spec.api_key_env.clone(),
            };
            let k = std::env::var(&env_name).unwrap_or_default();
            if k.is_empty() {
                return Err(RegistryError::MissingKey {
                    provider: m.provider.clone(),
                    env: env_name,
                });
            }
            k
        };
        let factory = make_provider_factory(spec.kind, key, spec.base_url.clone());
        let provider = factory(&m.model);
        self.cache
            .lock()
            .unwrap()
            .insert(m.clone(), Arc::clone(&provider));
        Ok(provider)
    }

    /// Build a sticky fallback chain: `primary` first, then each fallback in
    /// order. Resolving the primary must succeed; an unresolvable fallback is
    /// skipped (logged) rather than failing the whole chain — a degraded chain
    /// still beats none.
    pub fn chain_for(
        &self,
        primary: &ModelRef,
        fallbacks: &[ModelRef],
        on_change: Option<regent_providers::ActiveChangeFn>,
    ) -> Result<Arc<dyn ChatProvider>, RegistryError> {
        let mut chain: Vec<Arc<dyn ChatProvider>> = vec![self.provider_for(primary)?];
        for fb in fallbacks {
            match self.provider_for(fb) {
                Ok(p) => chain.push(p),
                Err(e) => tracing::warn!(fallback = %fb, %e, "skipping unresolvable fallback"),
            }
        }
        // Single primary, no resolvable fallbacks: return it directly (no need
        // to wrap one provider in a chain).
        if chain.len() == 1 {
            return Ok(chain.into_iter().next().unwrap());
        }
        let chat = FallbackChat::new(chain).map_err(|_| RegistryError::EmptyChain)?;
        let chat = match on_change {
            Some(cb) => chat.with_on_change(cb),
            None => chat,
        };
        Ok(Arc::new(chat) as Arc<dyn ChatProvider>)
    }

    /// Provider-aware parse of a model spec into a [`ModelRef`].
    /// - `"<provider>/<id>"` where `<provider>` is configured ⇒ that provider.
    /// - otherwise a provider that explicitly LISTS the spec in its `models`
    ///   wins (first by name for determinism) — pinning e.g. an OpenRouter-style
    ///   `"org/model"` id onto whatever provider happens to be primary sends it
    ///   somewhere that 404s it.
    /// - otherwise, if `default` is set ⇒ that provider with the whole spec as
    ///   the model id (so OpenRouter ids like `"anthropic/claude-…"` stay intact).
    /// - else `None`.
    #[must_use]
    pub fn resolve_model_str(&self, spec: &str, default: Option<&ModelRef>) -> Option<ModelRef> {
        if let Some((head, tail)) = spec.split_once('/')
            && self.specs.contains_key(head)
            && !tail.is_empty()
        {
            return Some(ModelRef::new(head, tail));
        }
        let mut listing: Vec<&String> = self
            .specs
            .iter()
            .filter(|(_, s)| s.models.iter().any(|m| m == spec))
            .map(|(name, _)| name)
            .collect();
        listing.sort();
        if let Some(name) = listing.first() {
            return Some(ModelRef::new((*name).clone(), spec));
        }
        default.map(|d| ModelRef::new(d.provider.clone(), spec))
    }
}

#[cfg(test)]
#[path = "provider_registry_tests.rs"]
mod tests;

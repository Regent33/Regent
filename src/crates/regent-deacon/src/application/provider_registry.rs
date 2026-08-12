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
use regent_providers::{ChatProvider, FallbackChat, FallbackHealth};
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
    health: Arc<FallbackHealth>,
}

impl ProviderRegistry {
    #[must_use]
    pub fn from_config(specs: &HashMap<String, ProviderSpec>) -> Self {
        Self {
            specs: specs.clone(),
            cache: Mutex::new(HashMap::new()),
            health: Arc::new(FallbackHealth::default()),
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
        let mut health_keys = vec![primary.to_string()];
        for fb in fallbacks {
            match self.provider_for(fb) {
                Ok(p) => {
                    chain.push(p);
                    health_keys.push(fb.to_string());
                }
                Err(e) => tracing::warn!(fallback = %fb, %e, "skipping unresolvable fallback"),
            }
        }
        // Single primary, no resolvable fallbacks: return it directly (no need
        // to wrap one provider in a chain).
        if chain.len() == 1 {
            return Ok(chain.into_iter().next().unwrap());
        }
        let chat = FallbackChat::with_shared_health(chain, health_keys, Arc::clone(&self.health))
            .map_err(|_| RegistryError::EmptyChain)?;
        let chat = match on_change {
            Some(cb) => chat.with_on_change(cb),
            None => chat,
        };
        Ok(Arc::new(chat) as Arc<dyn ChatProvider>)
    }

    /// Fallback candidates derived from the configured providers, for when the
    /// user set no explicit `agents_defaults.fallbacks`: every other configured
    /// (provider, model) pair — OTHER providers first (an independent failure
    /// domain, so a dead key/endpoint on the primary doesn't sink the fallbacks
    /// too), then the primary provider's remaining models (covers a single bad
    /// model, e.g. one that only ever returns private reasoning). Capped so a
    /// dead primary never serially pays many providers' timeouts; unresolvable
    /// ones (missing key) are skipped later by [`Self::chain_for`].
    #[must_use]
    pub fn auto_fallbacks(&self, exclude: &ModelRef) -> Vec<ModelRef> {
        const MAX: usize = 4;
        let mut specs: Vec<(&String, &ProviderSpec)> = self.specs.iter().collect();
        specs.sort_by(|a, b| a.0.cmp(b.0)); // deterministic order
        let (mut other, mut same) = (Vec::new(), Vec::new());
        for (name, spec) in specs {
            for model in &spec.models {
                if *name == exclude.provider && *model == exclude.model {
                    continue; // never fall back onto the very model that failed
                }
                let m = ModelRef::new(name.clone(), model.clone());
                if *name == exclude.provider {
                    same.push(m);
                } else {
                    other.push(m);
                }
            }
        }
        other.into_iter().chain(same).take(MAX).collect()
    }

    /// Provider-aware parse of a model spec into a [`ModelRef`], most
    /// authoritative rung first — each rung is a statement about the WHOLE
    /// spec, and only the last two derive anything from its shape:
    /// - `default` already IS this exact id ⇒ use it verbatim.
    /// - a provider explicitly LISTS the spec in its `models` ⇒ that provider,
    ///   with the spec intact (first by name for determinism). Written down by
    ///   the operator, so it outranks any inference from the string.
    /// - `"<provider>/<id>"` where `<provider>` is configured ⇒ that provider,
    ///   `<id>` as the model.
    /// - otherwise, if `default` is set ⇒ that provider with the whole spec as
    ///   the model id (so OpenRouter ids like `"anthropic/claude-…"` stay intact).
    /// - else `None`.
    #[must_use]
    pub fn resolve_model_str(&self, spec: &str, default: Option<&ModelRef>) -> Option<ModelRef> {
        // Some hosts put the vendor INSIDE the model id — NVIDIA NIM serves
        // "nvidia/nemotron-3-ultra-550b-a55b", where the prefix identifies the
        // model, not the provider. When the user also names that provider
        // `nvidia` (the obvious name), the split below ate the half that says
        // WHICH model and asked the host for a bare "nemotron-…" it has never
        // heard of. If the caller's default is already this exact id, it is an
        // authoritative resolution of the whole spec — string surgery cannot
        // improve on it, and re-deriving it 404'd every `model.review` turn.
        if let Some(resolved) = default.filter(|d| d.model == spec) {
            return Some(resolved.clone());
        }
        // An explicit `models:` entry is the same kind of authority the guard
        // above relies on: the operator wrote this id down against this
        // provider, so it is a statement about the whole string. It therefore
        // has to be checked BEFORE the split, not after — the split matches on
        // the provider NAME, and a host that puts its vendor inside the model id
        // (`nvidia/…` on NIM) collides with a provider the operator called
        // `nvidia`, which is the obvious name to give it. Ordered the other way,
        // the split answered first and asked the host for a bare
        // `nemotron-3-ultra-550b-a55b` it has never heard of — the same 404 the
        // guard above was added for, reached by a different route because that
        // guard only covers the case where the DEFAULT is already this exact id.
        // Titling and any other String-typed model setting took that route.
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
        if let Some((head, tail)) = spec.split_once('/')
            && self.specs.contains_key(head)
            && !tail.is_empty()
        {
            return Some(ModelRef::new(head, tail));
        }
        default.map(|d| ModelRef::new(d.provider.clone(), spec))
    }
}

#[cfg(test)]
#[path = "provider_registry_tests.rs"]
mod tests;

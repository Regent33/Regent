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
mod tests {
    use super::*;
    use crate::domain::config::ProviderKind;

    fn spec(kind: ProviderKind, key_env: &str, models: &[&str]) -> ProviderSpec {
        ProviderSpec {
            kind,
            base_url: None,
            api_key_env: key_env.to_owned(),
            models: models.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    fn registry(key_env: &str) -> ProviderRegistry {
        let mut specs = HashMap::new();
        specs.insert(
            "groq".to_owned(),
            spec(ProviderKind::Groq, key_env, &["llama-3.3-70b"]),
        );
        ProviderRegistry::from_config(&specs)
    }

    #[test]
    fn resolves_and_memoizes_a_known_provider() {
        // Unique env var name → no cross-test interference under parallel runs.
        let env = "REGENT_TEST_KEY_RESOLVES";
        unsafe { std::env::set_var(env, "secret") };
        let reg = registry(env);
        let m = ModelRef::new("groq", "llama-3.3-70b");
        let p1 = reg.provider_for(&m).unwrap();
        assert_eq!(p1.model(), "llama-3.3-70b");
        let p2 = reg.provider_for(&m).unwrap();
        assert!(Arc::ptr_eq(&p1, &p2), "second resolve is a cache hit");
        unsafe { std::env::remove_var(env) };
    }

    #[test]
    fn unknown_provider_is_a_typed_error() {
        // matches! on the Result: Arc<dyn ChatProvider> isn't Debug, so no unwrap_err.
        let reg = registry("REGENT_TEST_KEY_UNKNOWN");
        let res = reg.provider_for(&ModelRef::new("nope", "x"));
        assert!(matches!(res, Err(RegistryError::UnknownProvider(p)) if p == "nope"));
    }

    #[test]
    fn missing_key_is_a_typed_error() {
        // This env var is never set anywhere → MissingKey.
        let reg = registry("REGENT_TEST_KEY_DEFINITELY_UNSET");
        let res = reg.provider_for(&ModelRef::new("groq", "llama-3.3-70b"));
        assert!(
            matches!(res, Err(RegistryError::MissingKey { ref env, .. }) if env == "REGENT_TEST_KEY_DEFINITELY_UNSET")
        );
    }

    #[test]
    fn keyless_provider_resolves_with_an_empty_key() {
        // A provider with no `api_key_env` (local Ollama on localhost) is keyless
        // — it must resolve with NO env var set, not error as MissingKey.
        let mut specs = HashMap::new();
        specs.insert(
            "ollama-local".to_owned(),
            spec(ProviderKind::Ollama, "", &[]),
        );
        let reg = ProviderRegistry::from_config(&specs);
        assert!(
            reg.provider_for(&ModelRef::new("ollama-local", "llama3"))
                .is_ok(),
            "keyless provider resolves without any key"
        );
    }

    #[test]
    fn key_slot_pins_the_slotted_var_and_memoizes_separately() {
        let env = "REGENT_TEST_KEY_SLOTTED";
        unsafe {
            std::env::set_var(env, "key-one");
            std::env::set_var(format!("{env}_2"), "key-two");
        }
        let reg = registry(env);
        let base = ModelRef::new("groq", "llama-3.3-70b");
        // Same provider+model on slot 2 is a DIFFERENT chain link (multi-key
        // failover) — distinct cache entries, both resolvable.
        let p1 = reg.provider_for(&base).unwrap();
        let p2 = reg.provider_for(&base.clone().with_key_slot(2)).unwrap();
        assert!(!Arc::ptr_eq(&p1, &p2), "slots resolve independently");
        // An unset slot is the same typed error a missing base key produces,
        // naming the exact slotted var.
        let res = reg.provider_for(&base.with_key_slot(7));
        assert!(
            matches!(res, Err(RegistryError::MissingKey { ref env, .. }) if env == "REGENT_TEST_KEY_SLOTTED_7")
        );
        unsafe {
            std::env::remove_var(env);
            std::env::remove_var(format!("{env}_2"));
        }
    }

    #[test]
    fn chain_for_skips_a_fallback_whose_slot_is_unset() {
        let env = "REGENT_TEST_KEY_SLOTGAP";
        unsafe { std::env::set_var(env, "secret") };
        let reg = registry(env);
        let primary = ModelRef::new("groq", "llama-3.3-70b");
        // Slot 5 never set → that fallback is skipped, chain degrades to primary.
        let chain = reg
            .chain_for(&primary, &[primary.clone().with_key_slot(5)], None)
            .unwrap();
        assert_eq!(chain.model(), "llama-3.3-70b");
        unsafe { std::env::remove_var(env) };
    }

    #[test]
    fn chain_for_builds_a_fallback_when_fallbacks_resolve() {
        let env = "REGENT_TEST_KEY_CHAIN";
        unsafe { std::env::set_var(env, "secret") };
        let mut specs = HashMap::new();
        specs.insert("a".to_owned(), spec(ProviderKind::Groq, env, &["m1"]));
        specs.insert("b".to_owned(), spec(ProviderKind::Groq, env, &["m2"]));
        let reg = ProviderRegistry::from_config(&specs);
        let chain = reg
            .chain_for(&ModelRef::new("a", "m1"), &[ModelRef::new("b", "m2")], None)
            .unwrap();
        assert_eq!(chain.model(), "m1", "primary serves first");
        unsafe { std::env::remove_var(env) };
    }

    #[test]
    fn resolve_model_str_prefers_a_configured_provider_prefix() {
        let reg = registry("REGENT_TEST_KEY_PARSE");
        // "groq/" prefix is a configured provider → split.
        let m = reg.resolve_model_str("groq/llama-3.3-70b", None).unwrap();
        assert_eq!(m, ModelRef::new("groq", "llama-3.3-70b"));
        // No configured prefix, no default → None.
        assert!(
            reg.resolve_model_str("anthropic/claude-opus-4-8", None)
                .is_none()
        );
        // Bare/slashed id with a default → whole spec is the model id under the default provider.
        let dflt = ModelRef::new("openrouter", "x");
        let m = reg
            .resolve_model_str("anthropic/claude-opus-4-8", Some(&dflt))
            .unwrap();
        assert_eq!(m, ModelRef::new("openrouter", "anthropic/claude-opus-4-8"));
    }

    #[test]
    fn resolve_model_str_prefers_the_provider_that_lists_the_model() {
        // "or" LISTS the org-prefixed id; "local" is the primary. The spec must
        // resolve to "or", not get pinned onto the primary (which would 404).
        let mut specs = HashMap::new();
        specs.insert(
            "or".to_owned(),
            spec(ProviderKind::OpenRouter, "K", &["minimax/minimax-m3"]),
        );
        specs.insert(
            "local".to_owned(),
            spec(ProviderKind::Ollama, "K", &["minimax-m3"]),
        );
        let reg = ProviderRegistry::from_config(&specs);
        let primary = ModelRef::new("local", "minimax-m3");
        let m = reg
            .resolve_model_str("minimax/minimax-m3", Some(&primary))
            .unwrap();
        assert_eq!(m, ModelRef::new("or", "minimax/minimax-m3"));
        // A bare id listed by a provider resolves there too.
        let m = reg.resolve_model_str("minimax-m3", Some(&primary)).unwrap();
        assert_eq!(m, ModelRef::new("local", "minimax-m3"));
    }
}

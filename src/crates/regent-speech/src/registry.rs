//! Provider registry — built-ins always win.
//!
//! A direct port of Hermes's `transcription_registry` / `tts_registry`: a
//! name-keyed map of plugin-registered providers, with a reserved set of
//! built-in names that registration refuses to shadow. Generic over the
//! provider trait object (`dyn AsrProvider` / `dyn TtsProvider`) since both
//! kinds need identical bookkeeping; lookups are case- and
//! whitespace-insensitive, matching how the configured `*.provider` value is
//! normalized.
//!
//! Dispatch resolution order (enforced one layer up, in the deacon
//! composition root, not here): a config-declared `command`-type provider →
//! built-in → registered plugin. This module owns only the plugin tier and the
//! built-ins-always-win guard.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Names reserved for built-in ASR backends — **derived from the catalog**, so
/// the reserved set is exactly the set that actually dispatches.
///
/// This was previously a hand-written list ported from Hermes containing names
/// with no implementation behind them (`elevenlabs`, `minimax`, `gemini`,
/// `piper`, …). `voice.models` renders this list as the menu, so those names
/// were advertised and then rejected by the factory on selection. Deriving it
/// makes that failure mode unrepresentable.
#[must_use]
pub fn builtin_asr_providers() -> Vec<&'static str> {
    crate::catalog::asr_ids()
}

/// Names reserved for built-in TTS backends. See [`builtin_asr_providers`].
#[must_use]
pub fn builtin_tts_providers() -> Vec<&'static str> {
    crate::catalog::tts_ids()
}

/// Why a registration was refused.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RegistryError {
    /// The provider name was empty or whitespace-only.
    #[error("provider name must be a non-empty string")]
    EmptyName,
    /// The name collides with a reserved built-in — built-ins always win.
    #[error("provider '{0}' shadows a built-in name; pick a different name")]
    ShadowsBuiltin(String),
}

/// A thread-safe map of plugin providers keyed by normalized name, guarding the
/// reserved built-in names. `T` is the provider trait object
/// (`dyn AsrProvider` / `dyn TtsProvider`).
pub struct ProviderRegistry<T: ?Sized> {
    /// Label used only in log lines (e.g. `"ASR"` / `"TTS"`).
    kind: &'static str,
    builtins: HashSet<String>,
    providers: Mutex<HashMap<String, Arc<T>>>,
}

impl<T: ?Sized> ProviderRegistry<T> {
    /// Create a registry whose `builtins` are reserved against shadowing.
    #[must_use]
    pub fn new(kind: &'static str, builtins: &[&str]) -> Self {
        Self {
            kind,
            builtins: builtins.iter().map(|n| normalize(n)).collect(),
            providers: Mutex::new(HashMap::new()),
        }
    }

    /// True when `name` is a reserved built-in.
    #[must_use]
    pub fn is_builtin(&self, name: &str) -> bool {
        self.builtins.contains(&normalize(name))
    }

    /// Register a plugin provider under `name`. Rejects an empty name and any
    /// name shadowing a built-in (logged + returned, never stored).
    /// Re-registering the same name overwrites the previous entry — predictable
    /// for hot-reload/test loops.
    pub fn register(&self, name: &str, provider: Arc<T>) -> Result<(), RegistryError> {
        let key = normalize(name);
        if key.is_empty() {
            return Err(RegistryError::EmptyName);
        }
        if self.builtins.contains(&key) {
            tracing::warn!(
                kind = self.kind,
                provider = %key,
                "provider registration ignored: shadows a built-in (built-ins always win)"
            );
            return Err(RegistryError::ShadowsBuiltin(key));
        }
        let replaced = self
            .providers
            .lock()
            .expect("registry mutex poisoned")
            .insert(key.clone(), provider)
            .is_some();
        tracing::debug!(
            kind = self.kind,
            provider = %key,
            replaced,
            "provider registered"
        );
        Ok(())
    }

    /// Look up a registered plugin provider by name (normalized).
    #[must_use]
    pub fn get(&self, name: &str) -> Option<Arc<T>> {
        self.providers
            .lock()
            .expect("registry mutex poisoned")
            .get(&normalize(name))
            .cloned()
    }

    /// Registered plugin names, sorted. Does not include built-ins (those are
    /// dispatched natively, not stored here).
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .providers
            .lock()
            .expect("registry mutex poisoned")
            .keys()
            .cloned()
            .collect();
        names.sort();
        names
    }
}

/// Lowercase + trim — the canonical key form, matching how the configured
/// `*.provider` value is compared.
fn normalize(name: &str) -> String {
    name.trim().to_lowercase()
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;

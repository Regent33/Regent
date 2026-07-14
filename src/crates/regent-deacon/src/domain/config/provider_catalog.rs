//! Curated default model catalogs per provider KIND — the pickable list a
//! provider offers when its config `models:` is empty. This is a UI/discovery
//! convenience only: it is NEVER written back into config.yaml (the
//! `providers.models` op reads it live; `config.set` only ever persists the
//! dotted path it's handed). A user's own `models:` list always wins.
//!
//! OpenRouter slugs were verified against openrouter.ai/api/v1/models and the
//! per-org catalog pages on 2026-07-10; native-API ids come from each
//! provider's own conventions (`-latest` aliases where the provider offers
//! them). When an id is uncertain it's left out: an empty list falls back to
//! the free-text entry in the UI, which beats a stale id that 404s. `Ollama`
//! is empty on purpose — its catalog is whatever the user has pulled locally,
//! which only the machine knows. One static table > splitting; the file runs
//! past 200 lines because OpenRouter alone carries ~46 entries.

use super::model::ProviderSpec;
use super::provider_kind::ProviderKind;

/// Ollama's HOSTED catalog (ollama.com) — distinct from the local kind default
/// (empty: only the machine knows its pulls). Verified against
/// ollama.com/search?c=cloud on 2026-07-10. Applied when an `ollama`-kind
/// provider's base_url points at ollama.com.
pub const OLLAMA_CLOUD_MODELS: &[&str] = &[
    "glm-5.2",
    "glm-5.1",
    "glm-5",
    "kimi-k2.7-code",
    "kimi-k2.6",
    "kimi-k2.5",
    "minimax-m3",
    "minimax-m2.7",
    "minimax-m2.5",
    "deepseek-v4-pro",
    "deepseek-v4-flash",
    "qwen3.5",
    "gemma4",
    "nemotron-3-ultra",
    "nemotron-3-super",
];

impl ProviderSpec {
    /// The curated defaults this provider's KIND contributes to its pickable
    /// catalog: an `ollama`-kind provider pointed at ollama.com gets the HOSTED
    /// list; every other kind gets its own `default_models`.
    #[must_use]
    pub fn curated_defaults(&self) -> &'static [&'static str] {
        if self.kind == ProviderKind::Ollama
            && self
                .base_url
                .as_deref()
                .is_some_and(|u| u.contains("ollama.com"))
        {
            OLLAMA_CLOUD_MODELS
        } else {
            self.kind.default_models()
        }
    }

    /// Whether any catalog already offers `model` — the provider's own
    /// configured `models:` list or its kind's curated defaults. A model a
    /// user applies that neither offers is a CUSTOM id.
    #[must_use]
    pub fn offers(&self, model: &str) -> bool {
        self.models.iter().any(|m| m == model) || self.curated_defaults().contains(&model)
    }
}

#[cfg(test)]
#[path = "provider_catalog_tests.rs"]
mod tests;

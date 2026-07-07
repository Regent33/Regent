//! Builds the per-session provider factory from config. A factory (not a fixed
//! provider) lets `model.set` rebuild per session. `Anthropic` is the native
//! adapter; every other kind is the OpenAI-compatible adapter pointed at the
//! right base URL + api path (both from `ProviderKind::openai_base_path`). An
//! explicit `base_url` override always wins over the kind's default host.

use crate::domain::config::ProviderKind;
use crate::domain::contracts::ProviderFactory;
use regent_providers::{
    AnthropicChat, AnthropicChatConfig, ChatProvider, OpenAiCompatChat, OpenAiCompatChatConfig,
};
use std::sync::Arc;

/// Builds a factory that constructs a provider for a given model name. The
/// deacon still boots without a key (errors surface on the first call).
#[must_use]
pub fn make_provider_factory(
    kind: ProviderKind,
    api_key: String,
    base_url_override: Option<String>,
) -> ProviderFactory {
    Arc::new(move |model: &str| -> Arc<dyn ChatProvider> {
        match kind {
            ProviderKind::Anthropic => {
                // Empty base → AnthropicChatConfig fills in api.anthropic.com.
                let base = base_url_override.clone().unwrap_or_default();
                Arc::new(AnthropicChat::new(AnthropicChatConfig::new(
                    base,
                    api_key.clone(),
                    model.to_owned(),
                )))
            }
            other => {
                // Per-provider base + path; the config `base_url` overrides the
                // host but the kind still owns the api path (e.g. Gemini's
                // /chat/completions vs the standard /v1/chat/completions).
                let (default_base, api_path) = other.openai_base_path();
                let base = base_url_override
                    .clone()
                    .unwrap_or_else(|| default_base.to_owned());
                let mut cfg = OpenAiCompatChatConfig::new(base, api_key.clone(), model.to_owned());
                cfg.api_path = api_path.to_owned();
                Arc::new(OpenAiCompatChat::new(cfg))
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_kinds_map_to_their_hosts() {
        // Base/path now live on ProviderKind; spot-check the host halves here.
        assert_eq!(
            ProviderKind::Groq.openai_base_path().0,
            "https://api.groq.com/openai"
        );
        assert_eq!(
            ProviderKind::Ollama.openai_base_path().0,
            "http://localhost:11434"
        );
        // Openai + OpenRouter share the historical default.
        assert_eq!(
            ProviderKind::Openai.openai_base_path().0,
            "https://openrouter.ai/api"
        );
        assert_eq!(
            ProviderKind::OpenRouter.openai_base_path().0,
            "https://openrouter.ai/api"
        );
        // A non-standard path is respected (Gemini has no /v1 segment).
        assert_eq!(
            ProviderKind::Gemini.openai_base_path().1,
            "/chat/completions"
        );
    }

    #[test]
    fn factory_builds_a_provider_for_the_given_model() {
        let factory = make_provider_factory(ProviderKind::Groq, "k".into(), None);
        assert_eq!(factory("llama-3.3-70b").model(), "llama-3.3-70b");
    }

    #[test]
    fn base_url_override_wins_over_the_default() {
        // Just exercises the override branch — Anthropic with a custom base.
        let factory = make_provider_factory(
            ProviderKind::Anthropic,
            "k".into(),
            Some("https://x".into()),
        );
        assert_eq!(factory("claude").model(), "claude");
    }
}

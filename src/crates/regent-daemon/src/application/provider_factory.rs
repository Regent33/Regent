//! Builds the per-session provider factory from config. A factory (not a fixed
//! provider) lets `model.set` rebuild per session. `Anthropic` is the native
//! adapter; every other kind is the OpenAI-compatible adapter pointed at the
//! right base URL (an explicit `base_url` override always wins).

use crate::domain::config::ProviderKind;
use crate::domain::contracts::ProviderFactory;
use regent_providers::{
    AnthropicChat, AnthropicChatConfig, ChatProvider, OpenAiCompatChat, OpenAiCompatChatConfig,
};
use std::sync::Arc;

/// Default OpenAI-compatible base URL per kind. `Openai`/`OpenRouter` keep the
/// historical OpenRouter default; the named kinds use their own hosts.
fn default_base(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Groq => "https://api.groq.com/openai",
        ProviderKind::DeepSeek => "https://api.deepseek.com",
        ProviderKind::Together => "https://api.together.xyz",
        ProviderKind::Ollama => "http://localhost:11434",
        _ => "https://openrouter.ai/api",
    }
}

/// Builds a factory that constructs a provider for a given model name. The
/// daemon still boots without a key (errors surface on the first call).
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
                let base = base_url_override
                    .clone()
                    .unwrap_or_else(|| default_base(other).to_owned());
                Arc::new(OpenAiCompatChat::new(OpenAiCompatChatConfig::new(
                    base,
                    api_key.clone(),
                    model.to_owned(),
                )))
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_kinds_map_to_their_hosts() {
        assert_eq!(
            default_base(ProviderKind::Groq),
            "https://api.groq.com/openai"
        );
        assert_eq!(
            default_base(ProviderKind::DeepSeek),
            "https://api.deepseek.com"
        );
        assert_eq!(
            default_base(ProviderKind::Together),
            "https://api.together.xyz"
        );
        assert_eq!(default_base(ProviderKind::Ollama), "http://localhost:11434");
        // Openai + OpenRouter share the historical default.
        assert_eq!(
            default_base(ProviderKind::Openai),
            "https://openrouter.ai/api"
        );
        assert_eq!(
            default_base(ProviderKind::OpenRouter),
            "https://openrouter.ai/api"
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

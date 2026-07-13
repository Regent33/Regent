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
                let (base, api_path) = join_base_and_path(&base, api_path);
                let mut cfg = OpenAiCompatChatConfig::new(base, api_key.clone(), model.to_owned());
                cfg.api_path = api_path;
                Arc::new(OpenAiCompatChat::new(cfg))
            }
        }
    })
}

/// The OpenAI-style base URL (endpoint minus `/chat/completions`) the active
/// provider serves — what `vision_analyze`/`read_document` need to follow the
/// user's provider instead of a hardcoded default. `None` for Anthropic (its
/// wire shape is not OpenAI-compatible).
#[must_use]
pub fn openai_style_base(kind: ProviderKind, base_url_override: Option<&str>) -> Option<String> {
    if kind == ProviderKind::Anthropic {
        return None;
    }
    let (default_base, api_path) = kind.openai_base_path();
    let base = base_url_override.unwrap_or(default_base);
    let (base, path) = join_base_and_path(base, api_path);
    let endpoint = format!("{base}{path}");
    Some(
        endpoint
            .trim_end_matches('/')
            .trim_end_matches("/chat/completions")
            .to_owned(),
    )
}

/// Compose a (possibly user-pasted) base URL with the kind's api path without
/// doubling segments. Users configure bases like "https://openrouter.ai/api/v1"
/// or even the full endpoint; blindly appending "/v1/chat/completions" produced
/// ".../api/v1/v1/chat/completions" — an HTML 404 page from the host.
fn join_base_and_path(base: &str, api_path: &str) -> (String, String) {
    let base = base.trim_end_matches('/');
    if base.ends_with(api_path) {
        return (base.to_owned(), String::new());
    }
    if let Some(head) = api_path.strip_suffix("/chat/completions")
        && !head.is_empty()
        && base.ends_with(head)
    {
        return (base.to_owned(), "/chat/completions".to_owned());
    }
    (base.to_owned(), api_path.to_owned())
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
    fn base_and_path_compose_without_doubling_segments() {
        let path = "/v1/chat/completions";
        // Plain host → kind path appended untouched.
        assert_eq!(
            join_base_and_path("https://openrouter.ai/api", path),
            ("https://openrouter.ai/api".into(), path.to_owned())
        );
        // Base already carries "/v1" (the user's real config) → no double /v1.
        assert_eq!(
            join_base_and_path("https://openrouter.ai/api/v1", path),
            (
                "https://openrouter.ai/api/v1".into(),
                "/chat/completions".into()
            )
        );
        // Base IS the full endpoint → nothing appended.
        assert_eq!(
            join_base_and_path("https://x.dev/api/v1/chat/completions", path),
            (
                "https://x.dev/api/v1/chat/completions".into(),
                String::new()
            )
        );
        // Trailing slash tolerated.
        assert_eq!(
            join_base_and_path("https://ollama.com/", path),
            ("https://ollama.com".into(), path.to_owned())
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

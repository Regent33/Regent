//! Curated default model catalogs per provider KIND — the pickable list a
//! provider offers when its config `models:` is empty. This is a UI/discovery
//! convenience only: it is NEVER written back into config.yaml (the
//! `providers.models` op reads it live; `config.set` only ever persists the
//! dotted path it's handed). A user's own `models:` list always wins.
//!
//! IDs are intentionally short and conservative — only well-established,
//! currently-valid ids per kind. When an id is uncertain it's left out: an
//! empty list falls back to the free-text entry in the UI, which beats a stale
//! id that 404s. `Ollama` is empty on purpose — its catalog is whatever the
//! user has pulled locally, which only the machine knows.

use super::provider_kind::ProviderKind;

impl ProviderKind {
    /// Commonly-valid model ids for this kind, for the picker when the provider
    /// has no configured `models:` list. Empty is valid (free-text fallback).
    #[must_use]
    pub fn default_models(self) -> &'static [&'static str] {
        match self {
            Self::Anthropic => &[
                "claude-fable-5",
                "claude-opus-4-8",
                "claude-sonnet-5",
                "claude-haiku-4-5-20251001",
                "claude-sonnet-4-6",
            ],
            Self::Openai => &[
                "gpt-4o",
                "gpt-4o-mini",
                "gpt-4.1",
                "gpt-4.1-mini",
                "o3",
                "o4-mini",
            ],
            // OpenRouter ids are org-prefixed; these are stable, widely-used ones.
            Self::OpenRouter => &[
                "anthropic/claude-opus-4-8",
                "anthropic/claude-sonnet-4-6",
                "google/gemini-2.5-pro",
                "google/gemini-2.5-flash",
                "deepseek/deepseek-chat",
                "meta-llama/llama-3.3-70b-instruct",
            ],
            Self::Groq => &[
                "llama-3.3-70b-versatile",
                "llama-3.1-8b-instant",
                "openai/gpt-oss-120b",
                "moonshotai/kimi-k2-instruct",
            ],
            Self::DeepSeek => &["deepseek-chat", "deepseek-reasoner"],
            Self::Together => &[
                "meta-llama/Llama-3.3-70B-Instruct-Turbo",
                "deepseek-ai/DeepSeek-V3",
                "Qwen/Qwen2.5-72B-Instruct-Turbo",
            ],
            Self::Mistral => &[
                "mistral-large-latest",
                "mistral-small-latest",
                "codestral-latest",
            ],
            Self::Xai => &["grok-4", "grok-3", "grok-3-mini"],
            Self::Gemini => &["gemini-2.5-pro", "gemini-2.5-flash", "gemini-2.0-flash"],
            Self::Moonshot => &[
                "kimi-k2-0711-preview",
                "moonshot-v1-8k",
                "moonshot-v1-32k",
                "moonshot-v1-128k",
            ],
            // Zhipu / Z.AI GLM family.
            Self::Zhipu => &["glm-4.6", "glm-4.5", "glm-4.5-air"],
            // DashScope = Alibaba Qwen (compatible mode).
            Self::DashScope => &["qwen-max", "qwen-plus", "qwen-turbo"],
            Self::Fireworks => &[
                "accounts/fireworks/models/llama-v3p3-70b-instruct",
                "accounts/fireworks/models/deepseek-v3",
            ],
            Self::Cerebras => &["llama-3.3-70b", "llama3.1-8b", "qwen-3-235b-a22b-instruct"],
            Self::Perplexity => &[
                "sonar",
                "sonar-pro",
                "sonar-reasoning",
                "sonar-reasoning-pro",
            ],
            Self::Minimax => &["MiniMax-Text-01", "MiniMax-M1"],
            // Local: catalog is whatever the user has pulled — no fixed list.
            Self::Ollama => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ProviderKind;

    #[test]
    fn anthropic_lists_the_curated_ids_and_no_kind_panics() {
        // Exhaustive: every variant returns a (possibly empty) &'static slice.
        for kind in [
            ProviderKind::Anthropic,
            ProviderKind::Openai,
            ProviderKind::OpenRouter,
            ProviderKind::Groq,
            ProviderKind::DeepSeek,
            ProviderKind::Together,
            ProviderKind::Ollama,
            ProviderKind::Mistral,
            ProviderKind::Xai,
            ProviderKind::Gemini,
            ProviderKind::Moonshot,
            ProviderKind::Zhipu,
            ProviderKind::DashScope,
            ProviderKind::Fireworks,
            ProviderKind::Cerebras,
            ProviderKind::Perplexity,
            ProviderKind::Minimax,
        ] {
            let _ = kind.default_models();
        }
        let anthropic = ProviderKind::Anthropic.default_models();
        assert!(anthropic.contains(&"claude-opus-4-8"));
        assert!(anthropic.contains(&"claude-fable-5"));
        // Ollama is intentionally empty (local, machine-specific).
        assert!(ProviderKind::Ollama.default_models().is_empty());
    }
}

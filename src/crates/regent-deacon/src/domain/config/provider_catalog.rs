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

impl ProviderKind {
    /// Commonly-valid model ids for this kind, for the picker when the provider
    /// has no configured `models:` list. Empty is valid (free-text fallback).
    #[must_use]
    pub fn default_models(self) -> &'static [&'static str] {
        match self {
            Self::Anthropic => &[
                "claude-fable-5",
                "claude-opus-4-8",
                "claude-opus-4-7",
                "claude-opus-4-6",
                "claude-sonnet-5",
                "claude-sonnet-4-6",
                "claude-haiku-4-5",
            ],
            // GPT-5.6 family GA 2026-07-09 (Sol flagship — the bare "gpt-5.6"
            // alias routes to it — Terra balanced, Luna cost-optimized).
            Self::Openai => &[
                "gpt-5.6-sol",
                "gpt-5.6-terra",
                "gpt-5.6-luna",
                "gpt-5.5",
                "gpt-5.5-pro",
                "gpt-4.1",
                "gpt-4.1-mini",
                "gpt-4o",
                "gpt-4o-mini",
                "o3",
                "o4-mini",
            ],
            // OpenRouter ids are org-prefixed, exactly as the live catalog
            // serves them (dots and all: "claude-opus-4.8", "kimi-k2.7-code").
            Self::OpenRouter => &[
                "anthropic/claude-fable-5",
                "anthropic/claude-opus-4.8",
                "anthropic/claude-opus-4.8-fast",
                "anthropic/claude-opus-4.7-fast",
                "anthropic/claude-sonnet-5",
                "anthropic/claude-haiku-latest",
                "openai/gpt-5.6-sol",
                "openai/gpt-5.6-terra",
                "openai/gpt-5.6-luna",
                "openai/gpt-5.5-pro",
                "openai/gpt-5.5",
                "z-ai/glm-5.2",
                "z-ai/glm-5.1",
                "z-ai/glm-5v-turbo",
                "z-ai/glm-5-turbo",
                "mistralai/mistral-medium-3.5",
                "mistralai/mistral-small-2603",
                "mistralai/mistral-small-creative",
                "meta-llama/llama-4-maverick",
                "meta-llama/llama-4-scout",
                "meta-llama/llama-3.3-70b-instruct",
                "meta-llama/llama-3.3-8b-instruct",
                "google/gemini-3.5-flash",
                "google/gemini-3.1-flash-lite",
                "google/gemma-4-31b-it",
                "google/gemma-4-26b-a4b-it",
                "nvidia/nemotron-3-ultra-550b-a55b",
                "nvidia/nemotron-3-nano-omni-30b-a3b-reasoning",
                "moonshotai/kimi-k2.7-code",
                "moonshotai/kimi-k2.6",
                "moonshotai/kimi-k2.5",
                "moonshotai/kimi-k2-thinking",
                "minimax/minimax-m3",
                "minimax/minimax-m2.7",
                "minimax/minimax-m2.5",
                "qwen/qwen3.7-max",
                "qwen/qwen3.7-plus",
                "qwen/qwen3.6-max-preview",
                "deepseek/deepseek-v4-pro",
                "deepseek/deepseek-v4-flash",
                "deepseek/deepseek-v3.2",
                "deepseek/deepseek-v3.2-exp",
                "x-ai/grok-4.5",
                "x-ai/grok-4.3",
                "x-ai/grok-build-0.1",
                "microsoft/phi-4-mini-instruct",
                "perplexity/sonar-pro-search",
                "perplexity/sonar-reasoning-pro",
                "cohere/north-mini-code",
                "cohere/command-a",
                "cohere/command-r7b-12-2024",
                "cohere/command-r-plus-08-2024",
                "amazon/nova-2-lite-v1",
                "amazon/nova-pro-v1",
                "amazon/nova-lite-v1",
                "amazon/nova-micro-v1",
                "bytedance-seed/seed-2.0-lite",
                "bytedance-seed/seed-2.0-mini",
                "bytedance-seed/seed-1.6-flash",
                "bytedance-seed/seed-1.6",
                "baidu/ernie-4.5-300b-a47b",
                "baidu/ernie-4.5-vl-424b-a47b",
                "ai21/jamba-large-1.7",
                "ai21/jamba-mini-1.7",
                "tencent/hy3",
                "openrouter/fusion",
            ],
            Self::Groq => &[
                "llama-3.3-70b-versatile",
                "llama-3.1-8b-instant",
                "openai/gpt-oss-120b",
                "openai/gpt-oss-20b",
                "moonshotai/kimi-k2-instruct",
                "qwen/qwen3-32b",
            ],
            // DeepSeek's native API serves exactly two rolling aliases — both
            // always point at the newest release, so two IS the full catalog.
            Self::DeepSeek => &["deepseek-chat", "deepseek-reasoner"],
            Self::Together => &[
                "meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8",
                "meta-llama/Llama-3.3-70B-Instruct-Turbo",
                "deepseek-ai/DeepSeek-V3",
                "deepseek-ai/DeepSeek-R1",
                "Qwen/Qwen2.5-72B-Instruct-Turbo",
            ],
            // Mistral's `-latest` aliases roll forward with each release.
            Self::Mistral => &[
                "mistral-large-latest",
                "mistral-medium-latest",
                "mistral-small-latest",
                "codestral-latest",
                "magistral-medium-latest",
                "ministral-8b-latest",
            ],
            Self::Xai => &["grok-4.5", "grok-4.3", "grok-4", "grok-3", "grok-3-mini"],
            Self::Gemini => &[
                "gemini-3.5-flash",
                "gemini-3.1-flash-lite",
                "gemini-2.5-pro",
                "gemini-2.5-flash",
                "gemini-2.0-flash",
            ],
            Self::Moonshot => &[
                "kimi-latest",
                "kimi-k2-thinking",
                "kimi-k2-0711-preview",
                "moonshot-v1-128k",
                "moonshot-v1-32k",
                "moonshot-v1-8k",
            ],
            // Zhipu / Z.AI GLM family.
            Self::Zhipu => &["glm-5.2", "glm-5.1", "glm-4.6", "glm-4.5", "glm-4.5-air"],
            // DashScope = Alibaba Qwen (compatible mode) — rolling aliases.
            Self::DashScope => &[
                "qwen-max",
                "qwen-plus",
                "qwen-turbo",
                "qwen-long",
                "qwen-coder-plus",
            ],
            Self::Fireworks => &[
                "accounts/fireworks/models/llama4-maverick-instruct-basic",
                "accounts/fireworks/models/llama4-scout-instruct-basic",
                "accounts/fireworks/models/llama-v3p3-70b-instruct",
                "accounts/fireworks/models/deepseek-v3",
                "accounts/fireworks/models/qwen2p5-72b-instruct",
            ],
            Self::Cerebras => &[
                "llama-3.3-70b",
                "llama3.1-8b",
                "llama-4-scout-17b-16e-instruct",
                "gpt-oss-120b",
                "qwen-3-235b-a22b-instruct",
            ],
            Self::Perplexity => &[
                "sonar",
                "sonar-pro",
                "sonar-pro-search",
                "sonar-reasoning",
                "sonar-reasoning-pro",
                "sonar-deep-research",
            ],
            Self::Minimax => &["MiniMax-M3", "MiniMax-M2", "MiniMax-M1", "MiniMax-Text-01"],
            // NVIDIA NIM (build.nvidia.com) — org-prefixed ids, same slug shape
            // the OpenRouter list above uses for the nemotron family.
            Self::Nvidia => &[
                "z-ai/glm-5.2",
                "nvidia/nemotron-3-ultra-550b-a55b",
                "nvidia/nemotron-3-nano-omni-30b-a3b-reasoning",
                "nvidia/llama-3.3-nemotron-super-49b-v1",
                "nvidia/llama-3.1-nemotron-ultra-253b-v1",
                "nvidia/llama-3.1-nemotron-70b-instruct",
                "meta/llama-3.3-70b-instruct",
                "meta/llama-4-maverick-17b-128e-instruct",
                "deepseek-ai/deepseek-v4-flash",
                "deepseek-ai/deepseek-r1",
                "qwen/qwen2.5-coder-32b-instruct",
                "moonshotai/kimi-k2-instruct",
            ],
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
            ProviderKind::Nvidia,
        ] {
            let _ = kind.default_models();
        }
        let anthropic = ProviderKind::Anthropic.default_models();
        assert!(anthropic.contains(&"claude-opus-4-8"));
        assert!(anthropic.contains(&"claude-fable-5"));
        // Ollama is intentionally empty (local, machine-specific).
        assert!(ProviderKind::Ollama.default_models().is_empty());
    }

    #[test]
    fn every_remote_kind_offers_a_usable_catalog() {
        // The user-facing bar: every remote kind lists at least 5 pickable
        // models (DeepSeek's native API genuinely serves only 2 rolling
        // aliases), and OpenRouter carries the wide multi-org catalog.
        assert!(ProviderKind::OpenRouter.default_models().len() >= 60);
        for kind in [
            ProviderKind::Anthropic,
            ProviderKind::Openai,
            ProviderKind::Groq,
            ProviderKind::Together,
            ProviderKind::Mistral,
            ProviderKind::Xai,
            ProviderKind::Gemini,
            ProviderKind::Moonshot,
            ProviderKind::Zhipu,
            ProviderKind::DashScope,
            ProviderKind::Fireworks,
            ProviderKind::Cerebras,
            ProviderKind::Perplexity,
            ProviderKind::Nvidia,
        ] {
            assert!(kind.default_models().len() >= 5, "{kind:?} lists too few");
        }
    }
}

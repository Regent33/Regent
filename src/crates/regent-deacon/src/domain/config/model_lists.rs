//! Curated per-provider model catalogs (pure data). Split from
//! `provider_catalog.rs` (file-size rule).

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
            // Hosted: a real catalog, unlike the local daemon, because it is the
            // same list for everyone. Lives in provider_catalog so the existing
            // `kind: ollama` + `base_url: ollama.com` configs share it.
            Self::OllamaCloud => super::provider_catalog::OLLAMA_CLOUD_MODELS,
        }
    }
}

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
                // Fable 5.1 GA 2026-09-01. Mythos 5.1 is the same weights with
                // relaxed safeguards and is invitation-only (Project
                // Glasswing), so it is deliberately not listed here.
                "claude-fable-5-1",
                "claude-fable-5",
                "claude-opus-5",
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
                // GPT-6 Astra GA 2026-09-03: 1.05M window, the flagship for
                // long-horizon agentic/computer-use work. "-pro" is the
                // higher-compute tier of the same model.
                "gpt-6-astra",
                "gpt-6-astra-pro",
                "gpt-5.6-sol",
                "gpt-5.6-terra",
                "gpt-5.6-luna",
                "gpt-5.6-cyber",
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
            // Verified 2026-09-07 against the live /api/v1/models listing —
            // eleven curated ids had been retired upstream and now 404
            // ("*-fast" Claude variants, claude-haiku-latest, jamba, the
            // 300b ernie, phi-4-mini, mistral-medium-3.5's dotted spelling).
            Self::OpenRouter => &[
                "anthropic/claude-fable-5.1",
                "anthropic/claude-fable-5",
                "anthropic/claude-opus-5",
                "anthropic/claude-opus-4.8",
                "anthropic/claude-opus-4.7",
                "anthropic/claude-sonnet-5",
                "anthropic/claude-sonnet-4.6",
                "anthropic/claude-haiku-4.5",
                "openai/gpt-6-astra",
                "openai/gpt-6-astra-pro",
                "openai/gpt-5.6-sol",
                "openai/gpt-5.6-terra",
                "openai/gpt-5.6-terra-pro",
                "openai/gpt-5.6-luna",
                "openai/gpt-5.6-luna-pro",
                // Poolside Laguna S/XS 2.1 (2026-07-21/07-02): open-weight
                // agentic-coding MoE models; OpenRouter serves both a paid and
                // a free (":free", input/output may be used for training) tier.
                "poolside/laguna-s-2.1",
                "poolside/laguna-s-2.1:free",
                "poolside/laguna-xs-2.1",
                "poolside/laguna-xs-2.1:free",
                "z-ai/glm-5.3",
                "z-ai/glm-5.3-flash",
                "z-ai/glm-5.2",
                "z-ai/glm-5.1",
                "z-ai/glm-5v-turbo",
                "z-ai/glm-5-turbo",
                "mistralai/mistral-medium-3-5",
                "mistralai/mistral-small-2603",
                "mistralai/ministral-14b-2512",
                "meta/muse-spark-1.3",
                "meta/muse-glimmer-30b",
                "meta-llama/llama-4-maverick",
                "meta-llama/llama-4-scout",
                "meta-llama/llama-3.3-70b-instruct",
                "google/gemini-3.8-flash",
                "google/gemini-3.7-flash",
                "google/gemini-3.6-flash",
                "google/gemini-3.5-flash-lite",
                "google/gemma-4-31b-it",
                "google/gemma-4-26b-a4b-it",
                "nvidia/nemotron-3.5-lightning",
                "nvidia/nemotron-3-ultra-550b-a55b",
                "nvidia/nemotron-3-super-120b-a12b",
                "nvidia/nemotron-3-nano-30b-a3b",
                "moonshotai/kimi-k3",
                "moonshotai/kimi-k2.7-code",
                "moonshotai/kimi-k2.6",
                "moonshotai/kimi-k2.5",
                "moonshotai/kimi-k2-thinking",
                "minimax/minimax-m3",
                "minimax/minimax-m2.7",
                "minimax/minimax-m2.5",
                "qwen/qwen3.8-max-0902",
                "qwen/qwen3.8-flash",
                "qwen/qwen3.8-27b",
                "qwen/qwen3.7-max",
                "qwen/qwen3.7-plus",
                "qwen/qwen3.7-flash",
                "deepseek/deepseek-v4-pro",
                "deepseek/deepseek-v4-pro-0813",
                "deepseek/deepseek-v4-flash",
                "deepseek/deepseek-v4-flash-vision-exp",
                "deepseek/deepseek-v3.2",
                "deepseek/deepseek-v3.2-exp",
                "x-ai/grok-4.6",
                "x-ai/grok-4.5",
                "x-ai/grok-4.3",
                "x-ai/grok-build-0.1",
                "microsoft/phi-4",
                "perplexity/sonar-pro-search",
                "perplexity/sonar-reasoning-pro",
                "cohere/command-a",
                "cohere/command-r7b-12-2024",
                "cohere/command-r-plus-08-2024",
                "amazon/nova-premier-v1",
                "amazon/nova-2-lite-v1",
                "amazon/nova-pro-v1",
                "amazon/nova-lite-v1",
                "amazon/nova-micro-v1",
                "bytedance-seed/seed-2-1-turbo",
                "bytedance-seed/seed-2.0-code",
                "bytedance-seed/seed-2.0-lite",
                "bytedance-seed/seed-2.0-mini",
                "baidu/ernie-4.5-vl-424b-a47b",
                "tencent/hy4-preview",
                "tencent/hy3",
                "inception/mercury-2.5-preview",
                "thinkingmachines/inkling",
                "upstage/solar-pro4",
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
            // Grok 4.6 GA 2026-08-12 (500k window), the current flagship.
            Self::Xai => &[
                "grok-4.6",
                "grok-4.5",
                "grok-4.3",
                "grok-4",
                "grok-3",
                "grok-3-mini",
            ],
            Self::Gemini => &[
                // Gemini 3.8 Flash GA 2026-09-02, 3.7 Flash GA 2026-08-13.
                "gemini-3.8-flash",
                "gemini-3.7-flash",
                "gemini-3.6-flash",
                "gemini-3.5-flash",
                "gemini-3.5-flash-lite",
                "gemini-3.1-flash-lite",
                "gemini-2.5-pro",
                "gemini-2.5-flash",
                "gemini-2.0-flash",
            ],
            Self::Moonshot => &[
                // K3 (2026-07-16): the platform serves it under the bare "k3" id.
                "k3",
                "kimi-latest",
                "kimi-k2-thinking",
                "kimi-k2-0711-preview",
                "moonshot-v1-128k",
                "moonshot-v1-32k",
                "moonshot-v1-8k",
            ],
            // Zhipu / Z.AI GLM family.
            Self::Zhipu => &[
                "glm-5.3",
                "glm-5.3-flash",
                "glm-5.2",
                "glm-5.1",
                "glm-4.7",
                "glm-4.6",
                "glm-4.5",
                "glm-4.5-air",
            ],
            // DashScope = Alibaba Qwen (compatible mode) — rolling aliases.
            Self::DashScope => &[
                "qwen3.8-max",
                "qwen3.8-flash",
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
            // the OpenRouter list above uses for the nemotron family. Verified
            // 2026-09-07 against the live `integrate.api.nvidia.com/v1/models`
            // listing: eight previously-curated ids had been retired there
            // (glm-5.2, the llama-3.3/llama-4 meta slugs, deepseek-r1,
            // qwen2.5-coder, kimi-k2-instruct), so a picked id 404'd.
            Self::Nvidia => &[
                // Nemotron 3.5 Lightning (2026-08-11): 30B-A3B hybrid MoE for
                // long-running agents.
                "nvidia/nemotron-3.5-lightning-30b-a3b",
                "nvidia/nemotron-3-ultra-550b-a55b",
                "nvidia/nemotron-3-super-120b-a12b",
                "nvidia/nemotron-3-nano-omni-30b-a3b-reasoning",
                "nvidia/nemotron-nano-3-30b-a3b",
                "nvidia/llama-3.1-nemotron-ultra-253b-v1",
                "nvidia/llama-3.1-nemotron-70b-instruct",
                "deepseek-ai/deepseek-v4-pro-0813",
                "deepseek-ai/deepseek-v4-flash-0731",
                "moonshotai/kimi-k3",
                "moonshotai/kimi-k2.6",
                "minimaxai/minimax-m3",
                // Poolside Laguna XS 2.1 (2026-07-02): the larger Laguna S 2.1
                // still has no NIM-hosted endpoint, so it stays off this list.
                "poolside/laguna-xs-2.1",
                "meta/muse-glimmer-30b",
                "google/gemma-4-31b-it",
                "openai/gpt-oss-20b",
            ],
            // Local: pulled models arrive LIVE (providers.models queries the
            // daemon's /api/tags and they lead the list); these curated
            // `:cloud` tags fill in after — runnable through a signed-in local
            // daemon without pulling, so a fresh install still gets a pickable
            // catalog instead of a bare free-text field (owner ask 2026-07-17).
            Self::Ollama => super::provider_catalog::OLLAMA_LOCAL_CLOUD_TAGS,
            // Hosted: a real catalog, unlike the local daemon, because it is the
            // same list for everyone. Lives in provider_catalog so the existing
            // `kind: ollama` + `base_url: ollama.com` configs share it.
            Self::OllamaCloud => super::provider_catalog::OLLAMA_CLOUD_MODELS,
            // Open-weights hosts. They serve the same public checkpoints under
            // Hugging Face `org/model` slugs, so one shared list is honest for
            // all of them — per this module's rule, an id only appears where the
            // slug convention is the provider's own documented form.
            Self::DeepInfra
            | Self::Novita
            | Self::Nebius
            | Self::Hyperbolic
            | Self::SiliconFlow
            | Self::Chutes => OPEN_WEIGHTS_MODELS,
            Self::SambaNova => &[
                "DeepSeek-V3-0324",
                "Meta-Llama-3.3-70B-Instruct",
                "Llama-4-Maverick-17B-128E-Instruct",
                "Qwen3-32B",
            ],
            Self::Venice => &["venice-uncensored", "qwen3-235b", "llama-3.3-70b"],
            Self::Cohere => &["command-a-03-2025", "command-r-plus", "command-r7b"],
            Self::GitHubModels => &[
                "openai/gpt-4.1",
                "openai/gpt-4o",
                "openai/o4-mini",
                "meta/Llama-3.3-70B-Instruct",
                "mistral-ai/mistral-large-2411",
            ],
            // Servers you run: only the machine knows what is loaded, exactly
            // like local Ollama. Empty ⇒ the picker's free-text field, which
            // beats guessing at someone else's checkpoint.
            Self::LmStudio | Self::LlamaCpp | Self::Vllm | Self::LiteLlm => &[],
        }
    }
}

/// The open-weights checkpoints the multi-model hosts all serve, in Hugging
/// Face `org/model` form. Shared rather than duplicated six times: the whole
/// point of these providers is that they carry the same public models.
const OPEN_WEIGHTS_MODELS: &[&str] = &[
    "deepseek-ai/DeepSeek-V3",
    "deepseek-ai/DeepSeek-R1",
    "Qwen/Qwen3-235B-A22B",
    "Qwen/Qwen3-32B",
    "Qwen/Qwen2.5-Coder-32B-Instruct",
    "meta-llama/Llama-3.3-70B-Instruct",
    "meta-llama/Llama-4-Maverick-17B-128E-Instruct",
    "mistralai/Mistral-Small-24B-Instruct-2501",
    "openai/gpt-oss-120b",
];

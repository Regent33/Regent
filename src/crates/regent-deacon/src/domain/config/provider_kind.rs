//! `ProviderKind` — the wire protocol the deacon speaks to, and everything that
//! differs per provider: the `REGENT_PROVIDER` parse, the conventional key env
//! var, key resolution, and the OpenAI-compatible base URL + api path.
//!
//! Adding a provider = one enum variant + one line in each `match` here. Every
//! variant except `Anthropic` (native Messages API) is OpenAI-compatible and
//! differs only by `(base_url, api_path)` — several providers do NOT use the
//! standard `/v1/chat/completions` path, so both halves are encoded here.

use serde::{Deserialize, Serialize};

/// How many numbered key slots we probe for one provider: slot 1 is the
/// unsuffixed base var, slots 2..=N are `<BASE>_2` … `<BASE>_N`. Shared with
/// `env.*` so the settable/list surface agrees with what the runtime reads.
pub const MAX_KEY_SLOTS: usize = 8;

/// Which provider the deacon speaks to. `Anthropic` uses the native Messages
/// API; every other variant is an OpenAI-compatible endpoint differing only by
/// base URL + api path (both overridable — `base_url` via config). `Openai`
/// keeps its historical OpenRouter default for back-compat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    #[default]
    Anthropic,
    Openai,
    OpenRouter,
    Groq,
    DeepSeek,
    Together,
    Ollama,
    /// Ollama's hosted service. Same wire protocol as local Ollama, different
    /// endpoint and a key — precisely the relationship Openai/OpenRouter and
    /// Groq/DeepSeek already have, so it earns a variant rather than living on
    /// as a local `ollama` entry someone remembered to repoint at ollama.com.
    /// Renamed explicitly: the lowercase default would be `ollamacloud`.
    #[serde(rename = "ollama-cloud")]
    OllamaCloud,
    Mistral,
    Xai,
    Gemini,
    Moonshot,
    Zhipu,
    DashScope,
    Fireworks,
    Cerebras,
    Perplexity,
    Minimax,
    Nvidia,
}

impl ProviderKind {
    /// Every supported kind, in menu order (Anthropic first, local Ollama
    /// last — hosted Ollama sits with the other hosted services, next to it).
    /// The setup wizard's provider picker is generated from this, so adding an
    /// enum variant automatically reaches onboarding.
    pub const ALL: [Self; 19] = [
        Self::Anthropic,
        Self::Openai,
        Self::OpenRouter,
        Self::Groq,
        Self::DeepSeek,
        Self::Together,
        Self::Mistral,
        Self::Xai,
        Self::Gemini,
        Self::Moonshot,
        Self::Zhipu,
        Self::DashScope,
        Self::Fireworks,
        Self::Cerebras,
        Self::Perplexity,
        Self::Minimax,
        Self::Nvidia,
        Self::OllamaCloud,
        Self::Ollama,
    ];

    /// The lowercase wire name (the `serde` form `parse` accepts back).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::Openai => "openai",
            Self::OpenRouter => "openrouter",
            Self::Groq => "groq",
            Self::DeepSeek => "deepseek",
            Self::Together => "together",
            Self::Ollama => "ollama",
            Self::OllamaCloud => "ollama-cloud",
            Self::Mistral => "mistral",
            Self::Xai => "xai",
            Self::Gemini => "gemini",
            Self::Moonshot => "moonshot",
            Self::Zhipu => "zhipu",
            Self::DashScope => "dashscope",
            Self::Fireworks => "fireworks",
            Self::Cerebras => "cerebras",
            Self::Perplexity => "perplexity",
            Self::Minimax => "minimax",
            Self::Nvidia => "nvidia",
        }
    }

    /// Parse a lowercase provider name (the `serde` wire form). `None` for an
    /// unknown value so callers can keep their configured fallback.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "anthropic" => Self::Anthropic,
            "openai" => Self::Openai,
            "openrouter" => Self::OpenRouter,
            "groq" => Self::Groq,
            "deepseek" => Self::DeepSeek,
            "together" => Self::Together,
            "ollama" => Self::Ollama,
            "ollama-cloud" => Self::OllamaCloud,
            "mistral" => Self::Mistral,
            "xai" => Self::Xai,
            "gemini" => Self::Gemini,
            "moonshot" => Self::Moonshot,
            "zhipu" => Self::Zhipu,
            "dashscope" => Self::DashScope,
            "fireworks" => Self::Fireworks,
            "cerebras" => Self::Cerebras,
            "perplexity" => Self::Perplexity,
            "minimax" => Self::Minimax,
            "nvidia" => Self::Nvidia,
            _ => return None,
        })
    }

    /// Parses the `REGENT_PROVIDER` env override; unknown values keep `fallback`.
    #[must_use]
    pub fn from_env_or(fallback: Self) -> Self {
        std::env::var("REGENT_PROVIDER")
            .ok()
            .and_then(|v| Self::parse(v.trim()))
            .unwrap_or(fallback)
    }

    /// The conventional env var holding this provider's API key.
    #[must_use]
    pub fn key_env_var(self) -> &'static str {
        match self {
            Self::Anthropic => "ANTHROPIC_API_KEY",
            Self::Openai => "OPENAI_API_KEY",
            Self::OpenRouter => "OPENROUTER_API_KEY",
            Self::Groq => "GROQ_API_KEY",
            Self::DeepSeek => "DEEPSEEK_API_KEY",
            Self::Together => "TOGETHER_API_KEY",
            // Local Ollama is normally keyless (the registry treats an empty
            // `api_key_env` as such); the hosted service always needs this one.
            Self::Ollama | Self::OllamaCloud => "OLLAMA_API_KEY",
            Self::Mistral => "MISTRAL_API_KEY",
            Self::Xai => "XAI_API_KEY",
            Self::Gemini => "GEMINI_API_KEY",
            Self::Moonshot => "MOONSHOT_API_KEY",
            Self::Zhipu => "ZHIPU_API_KEY",
            Self::DashScope => "DASHSCOPE_API_KEY",
            Self::Fireworks => "FIREWORKS_API_KEY",
            Self::Cerebras => "CEREBRAS_API_KEY",
            Self::Perplexity => "PERPLEXITY_API_KEY",
            Self::Minimax => "MINIMAX_API_KEY",
            Self::Nvidia => "NVIDIA_API_KEY",
        }
    }

    /// Resolve the API key: this provider's own env var wins, else the generic
    /// `REGENT_API_KEY`. So an `ollama` main provider uses `OLLAMA_API_KEY`
    /// instead of being wrongly handed a generic key belonging to someone else.
    ///
    /// Multiple keys per provider: the base var is slot 1; if it's unset-or-empty
    /// we fall through to `<BASE>_2`, `<BASE>_3`, … (first non-empty wins). This
    /// is failover-on-startup only — the chosen key is fixed for the process.
    /// CEILING: there is NO per-request rotation. If slot 1 is set but gets
    /// rate-limited mid-session we do not hop to `_2`; doing that would mean
    /// threading a live key selector through every request path, which this
    /// deliberately avoids.
    #[must_use]
    pub fn resolve_key(self) -> String {
        let base = self.key_env_var();
        for slot in 1..=MAX_KEY_SLOTS {
            // Slot 1 is the unsuffixed base; slots 2..=N are `<BASE>_2`, `<BASE>_3`, …
            let var = if slot == 1 {
                base.to_owned()
            } else {
                format!("{base}_{slot}")
            };
            if let Ok(v) = std::env::var(&var)
                && !v.trim().is_empty()
            {
                return v;
            }
        }
        // Generic fallback last, so any provider-specific key always wins.
        if let Ok(v) = std::env::var("REGENT_API_KEY")
            && !v.trim().is_empty()
        {
            return v;
        }
        String::new()
    }

    /// The OpenAI-compatible `(base_url, api_path)` for this provider. The final
    /// endpoint is `base_url + api_path`. Most use `/v1/chat/completions`, but
    /// Gemini/Zhipu/Perplexity mount chat-completions at a different path — so
    /// the path is per-provider, not a global constant. `Anthropic` returns its
    /// own host but the factory routes it to the native adapter, not this.
    #[must_use]
    pub fn openai_base_path(self) -> (&'static str, &'static str) {
        const CHAT: &str = "/v1/chat/completions";
        match self {
            Self::Anthropic => ("https://api.anthropic.com", CHAT),
            // Openai + OpenRouter share the historical OpenRouter default.
            Self::Openai | Self::OpenRouter => ("https://openrouter.ai/api", CHAT),
            Self::Groq => ("https://api.groq.com/openai", CHAT),
            Self::DeepSeek => ("https://api.deepseek.com", CHAT),
            Self::Together => ("https://api.together.xyz", CHAT),
            Self::Ollama => ("http://localhost:11434", CHAT),
            // Ollama's hosted service speaks the same OpenAI-compatible surface
            // as the local daemon, just off-machine and behind a key.
            Self::OllamaCloud => ("https://ollama.com", CHAT),
            Self::Mistral => ("https://api.mistral.ai", CHAT),
            Self::Xai => ("https://api.x.ai", CHAT),
            // Gemini's OpenAI-compat surface mounts chat under /v1beta/openai.
            Self::Gemini => (
                "https://generativelanguage.googleapis.com/v1beta/openai",
                "/chat/completions",
            ),
            Self::Moonshot => ("https://api.moonshot.ai", CHAT),
            // Zhipu (GLM/Z.AI) mounts under /api/paas/v4, no /v1.
            Self::Zhipu => ("https://open.bigmodel.cn/api/paas/v4", "/chat/completions"),
            Self::DashScope => ("https://dashscope-intl.aliyuncs.com/compatible-mode", CHAT),
            Self::Fireworks => ("https://api.fireworks.ai/inference", CHAT),
            Self::Cerebras => ("https://api.cerebras.ai", CHAT),
            // Perplexity's endpoint has no /v1 segment.
            Self::Perplexity => ("https://api.perplexity.ai", "/chat/completions"),
            Self::Minimax => ("https://api.minimax.io", CHAT),
            // NVIDIA NIM (build.nvidia.com) — OpenAI-compatible hosted endpoint.
            Self::Nvidia => ("https://integrate.api.nvidia.com", CHAT),
        }
    }
}

#[cfg(test)]
#[path = "provider_kind_tests.rs"]
mod tests;

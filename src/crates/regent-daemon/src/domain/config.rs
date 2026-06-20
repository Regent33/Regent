//! Daemon configuration schema. All fields have defaults so a minimal (or
//! missing) config.yaml always produces a working config; unknown keys are a
//! hard error so a typo never silently falls back to a default.

use serde::{Deserialize, Serialize};

pub const CURRENT_CONFIG_VERSION: u32 = 1;

/// Full daemon configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DaemonConfig {
    /// Bumped when the schema changes. Missing keys are filled with defaults
    /// (additive reconcile — same pattern as store schema versions).
    #[serde(rename = "_config_version")]
    pub config_version: u32,
    pub model: ModelConfig,
    pub context: ContextConfig,
    pub memory: MemoryConfig,
    pub cron: CronConfig,
    pub board: BoardConfig,
    pub http: HttpConfig,
    pub tools: ToolsConfig,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            config_version: CURRENT_CONFIG_VERSION,
            model: ModelConfig::default(),
            context: ContextConfig::default(),
            memory: MemoryConfig::default(),
            cron: CronConfig::default(),
            board: BoardConfig::default(),
            http: HttpConfig::default(),
            tools: ToolsConfig::default(),
        }
    }
}

/// Tool exposure. `disabled` names are filtered out of every session's catalog
/// (`tools disable <name>`), so the model never sees them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ToolsConfig {
    pub disabled: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModelConfig {
    pub default: String,
    /// Wire protocol: "anthropic" (native Messages API, prompt-cache
    /// breakpoints) or "openai" (any OpenAI-compatible endpoint).
    pub provider: ProviderKind,
    /// Overrides the provider base URL (None = provider's own default).
    pub base_url: Option<String>,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            default: "claude-sonnet-4-6".to_owned(),
            provider: ProviderKind::default(),
            base_url: None,
        }
    }
}

/// Which provider the daemon speaks to. `Anthropic` uses the native Messages
/// API; every other variant is an OpenAI-compatible endpoint differing only by
/// base URL (overridable via `base_url`). `Openai` keeps its historical
/// OpenRouter default for back-compat; the named variants default to their own
/// hosts.
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
}

impl ProviderKind {
    /// Parses the `REGENT_PROVIDER` env override; unknown values keep the
    /// configured value (returned via the fallback).
    #[must_use]
    pub fn from_env_or(fallback: Self) -> Self {
        match std::env::var("REGENT_PROVIDER").as_deref() {
            Ok("anthropic") => Self::Anthropic,
            Ok("openai") => Self::Openai,
            Ok("openrouter") => Self::OpenRouter,
            Ok("groq") => Self::Groq,
            Ok("deepseek") => Self::DeepSeek,
            Ok("together") => Self::Together,
            Ok("ollama") => Self::Ollama,
            _ => fallback,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContextConfig {
    pub max_tokens: u32,
    pub trigger_fraction: f32,
    pub protect_last_n: usize,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self { max_tokens: 200_000, trigger_fraction: 0.85, protect_last_n: 10 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MemoryConfig {
    /// Base directory for skills, cron jobs, and state.db.
    /// Tilde is expanded at runtime.
    pub home: String,
    /// Enable the local ONNX semantic (vector) lane of memory retrieval.
    /// When true (default) the daemon loads the embedding model on boot and
    /// fuses vector recall with FTS + graph; if the model can't load, memory
    /// degrades to FTS + graph rather than failing.
    pub embeddings: bool,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self { home: "~/.regent".to_owned(), embeddings: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CronConfig {
    pub tick_interval_secs: u64,
}

impl Default for CronConfig {
    fn default() -> Self {
        Self { tick_interval_secs: 30 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BoardConfig {
    /// Opt-in: when true, the daemon auto-runs `todo` tasks on the default
    /// board through an agent. **Off by default** — autonomous execution (and
    /// its token spend) is never enabled silently. Boards still default to
    /// `human` review, so even when enabled nothing self-completes unless a
    /// board's policy says so.
    pub enabled: bool,
    /// Seconds between dispatch ticks.
    pub tick_interval_secs: u64,
    /// Most tasks dispatched per tick (so one busy board can't starve the loop).
    pub max_per_tick: usize,
}

impl Default for BoardConfig {
    fn default() -> Self {
        Self { enabled: false, tick_interval_secs: 15, max_per_tick: 4 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HttpConfig {
    /// Opt-in REST ingress (`/health` + bearer-auth `/v1/chat`). **Off by
    /// default** — the daemon's primary transport is stdio JSON-RPC.
    pub enabled: bool,
    /// Listen address. Defaults to loopback so it is never world-exposed by
    /// accident; bind to `0.0.0.0:..` deliberately to face the network.
    pub bind: String,
    /// Bearer token required on `/v1/chat`. Empty disables the listener
    /// (deny-by-default — never serve the REST surface unauthenticated).
    pub token: String,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self { enabled: false, bind: "127.0.0.1:7878".to_owned(), token: String::new() }
    }
}

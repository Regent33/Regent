//! Deacon configuration schema. All fields have defaults so a minimal (or
//! missing) config.yaml always produces a working config; unknown keys are a
//! hard error so a typo never silently falls back to a default.

use regent_kernel::ModelRef;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const CURRENT_CONFIG_VERSION: u32 = 1;

/// Full deacon configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DeaconConfig {
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
    pub speech: SpeechConfig,
    /// Named providers usable simultaneously (multi-provider; each keyed
    /// independently). Empty = today's single-provider behavior (the `model`
    /// section + env). A named-agent `model` of `"<provider>/<id>"` resolves
    /// through this map; a bare id falls back to `agents_defaults.primary`.
    pub providers: HashMap<String, ProviderSpec>,
    /// Per-agent model defaults applied when a named agent runs through the
    /// provider registry (primary + an ordered fallback chain).
    pub agents_defaults: AgentsDefaults,
    /// Named Mixture-of-Models groups (§B). Each maps a name → proposer model
    /// specs + an aggregator. Empty = no MoM groups. Run via `mom.run`.
    pub mom: HashMap<String, MomGroupConfig>,
    pub constitution: ConstitutionConfig,
}

impl Default for DeaconConfig {
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
            speech: SpeechConfig::default(),
            providers: HashMap::new(),
            agents_defaults: AgentsDefaults::default(),
            mom: HashMap::new(),
            constitution: ConstitutionConfig::default(),
        }
    }
}

/// The constitutional values layer (character + hard boundaries, shipped in
/// `regent-agent`). **Off by default** — `regent setup` offers the opt-in;
/// when enabled the deacon seeds the `constitution` persona row from the
/// shipped document on boot and every session's prompt renders it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConstitutionConfig {
    pub enabled: bool,
}

/// One Mixture-of-Models group (§B): proposer model specs answered in parallel,
/// then `aggregator` synthesizes them. Specs are `"provider/model"` (or a bare
/// id resolved against `agents_defaults.primary`) — resolved through the
/// provider registry at run time. `max_proposers` 0 = the runner default (3).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct MomGroupConfig {
    pub proposers: Vec<String>,
    pub aggregator: String,
    pub max_proposers: usize,
}

/// One configured provider: a wire protocol (`kind`), an optional endpoint
/// override, the env var holding its key, and the model ids it serves. One
/// `api_key_env` serves every model in `models` (multi-model-per-key — §3).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct ProviderSpec {
    pub kind: ProviderKind,
    /// Override the wire base URL; `None` = the kind's own default.
    pub base_url: Option<String>,
    /// Env var name holding the API key (read at registry build — never the
    /// key itself, so secrets stay out of config and version control).
    pub api_key_env: String,
    /// Model ids this provider serves — the catalog `model.list` merges in.
    pub models: Vec<String>,
}

/// Per-agent model defaults: the primary model and an ordered fallback chain
/// applied to every named-agent provider built through the registry.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct AgentsDefaults {
    pub primary: Option<ModelRef>,
    pub fallbacks: Vec<ModelRef>,
}

/// Tool exposure. `disabled` names are filtered out of every session's catalog
/// (`tools disable <name>`), so the model never sees them. `deferred` names
/// stay executable but their schemas are withheld from every request until
/// loaded via `load_tools` — the token-efficiency lever: rare tools stop
/// costing their full schema on every model call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ToolsConfig {
    pub disabled: Vec<String>,
    pub deferred: Vec<String>,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            disabled: Vec::new(),
            // Rare, schema-heavy tools; override with `tools.deferred: []`.
            deferred: [
                "manage_keys",
                "image_generation",
                "video_analyze",
                "play",
                "control_app",
                "kanban",
                "update_persona",
                "skill_manage",
                "move_file",
                "copy_file",
                "delete_file",
                "send_file",
            ]
            .map(String::from)
            .to_vec(),
        }
    }
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

/// Which provider the deacon speaks to. `Anthropic` uses the native Messages
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
        Self {
            max_tokens: 200_000,
            trigger_fraction: 0.85,
            protect_last_n: 10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MemoryConfig {
    /// Base directory for skills, cron jobs, and state.db.
    /// Tilde is expanded at runtime.
    pub home: String,
    /// Enable the local ONNX semantic (vector) lane of memory retrieval.
    /// When true (default) the deacon loads the embedding model on boot and
    /// fuses vector recall with FTS + graph; if the model can't load, memory
    /// degrades to FTS + graph rather than failing.
    pub embeddings: bool,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            home: "~/.regent".to_owned(),
            embeddings: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CronConfig {
    pub tick_interval_secs: u64,
}

impl Default for CronConfig {
    fn default() -> Self {
        Self {
            tick_interval_secs: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BoardConfig {
    /// Opt-in: when true, the deacon auto-runs `todo` tasks on the default
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
        Self {
            enabled: false,
            tick_interval_secs: 15,
            max_per_tick: 4,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HttpConfig {
    /// Opt-in REST ingress (`/health` + bearer-auth `/v1/chat`). **Off by
    /// default** — the deacon's primary transport is stdio JSON-RPC.
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
        Self {
            enabled: false,
            bind: "127.0.0.1:7878".to_owned(),
            token: String::new(),
        }
    }
}

/// Voice/vision stack. **Off by default** — a fresh deacon loads or downloads
/// no speech model until `regent voice setup` flips `enabled` (same opt-in
/// shape as `HttpConfig`/`BoardConfig`). Defaults are Qwen3-ASR/Qwen3-TTS,
/// swappable to any provider/model. See `regent-speech`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SpeechConfig {
    pub enabled: bool,
    /// Where local ASR/TTS weights live. Default `tts-asr-local-models` (relative
    /// to where Regent runs — i.e. the repo); set an absolute path to override.
    /// `~` is expanded at runtime.
    pub models_dir: String,
    pub asr: AsrConfig,
    pub tts: TtsConfig,
    pub vision: VisionConfig,
    pub call: CallConfig,
}

impl Default for SpeechConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            models_dir: "tts-asr-local-models".to_owned(),
            asr: AsrConfig::default(),
            tts: TtsConfig::default(),
            vision: VisionConfig::default(),
            call: CallConfig::default(),
        }
    }
}

/// One downloadable weight file for a local model: filename, source URL, and
/// expected SHA-256 (empty = trust as downloaded). `regent voice setup` fetches
/// these into `models_dir` via the model manager. Empty `weights` ⇒ nothing to
/// download (a hosted provider, or a localhost server you run yourself).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct WeightFile {
    pub name: String,
    pub url: String,
    pub sha256: String,
}

/// Speech-to-text. Defaults to **local** Qwen3-ASR. When `weights` are set,
/// `voice setup` downloads them into `models_dir` and a local runtime serves
/// them; otherwise `base_url` points at an OpenAI-compatible endpoint (hosted,
/// or a localhost server). `provider` is a registry name; `language` is a
/// BCP-47 hint or `"auto"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AsrConfig {
    pub provider: String,
    pub model: String,
    pub language: String,
    /// Override the OpenAI-compatible base URL. Empty = the provider default
    /// (local ⇒ a localhost server; qwen/groq/openai ⇒ their hosted endpoint).
    pub base_url: String,
    /// Weight files to download for a local model. Empty = nothing to fetch.
    pub weights: Vec<WeightFile>,
}

impl Default for AsrConfig {
    fn default() -> Self {
        Self {
            provider: "local".to_owned(),
            model: "qwen3-asr-1.7b".to_owned(),
            language: "auto".to_owned(),
            base_url: String::new(),
            weights: Vec::new(),
        }
    }
}

/// Text-to-speech. Defaults to **local** Qwen3-TTS (see [`AsrConfig`]). `format`
/// is the output container (`opus` for voice bubbles).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TtsConfig {
    pub provider: String,
    pub model: String,
    pub voice: String,
    pub format: String,
    /// Override the OpenAI-compatible base URL (see [`AsrConfig::base_url`]).
    pub base_url: String,
    /// Weight files to download for a local model. Empty = nothing to fetch.
    pub weights: Vec<WeightFile>,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            provider: "local".to_owned(),
            model: "qwen3-tts-1.7b".to_owned(),
            voice: "default".to_owned(),
            format: "opus".to_owned(),
            base_url: String::new(),
            weights: Vec::new(),
        }
    }
}

/// Vision routing. `input_mode` is `auto|native|text`; `provider`/`model`
/// select the vision/aux model (`auto`/empty = the main multimodal model).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VisionConfig {
    pub input_mode: String,
    pub provider: String,
    pub model: String,
    /// Seconds to wait when fetching a remote image before giving up.
    pub download_timeout: u64,
}

impl Default for VisionConfig {
    fn default() -> Self {
        Self {
            input_mode: "auto".to_owned(),
            provider: "auto".to_owned(),
            model: String::new(),
            download_timeout: 30,
        }
    }
}

/// Model tiering for spoken turns. `fast_model` (e.g. a `*-flash` model)
/// answers quick conversational turns; empty = always use the main model.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CallConfig {
    pub fast_model: String,
}

#[cfg(test)]
mod speech_config_tests {
    use super::*;

    #[test]
    fn speech_is_disabled_by_default_with_qwen3() {
        let c = DeaconConfig::default();
        assert!(!c.speech.enabled);
        assert_eq!(c.speech.asr.model, "qwen3-asr-1.7b");
        assert_eq!(c.speech.tts.model, "qwen3-tts-1.7b");
        assert!(c.speech.asr.weights.is_empty()); // nothing downloads by default
        assert_eq!(c.speech.tts.format, "opus");
        assert_eq!(c.speech.vision.input_mode, "auto");
        assert!(c.speech.call.fast_model.is_empty());
    }

    #[test]
    fn config_without_a_speech_section_fills_defaults() {
        // serde(default): a config that predates speech still parses and the
        // section defaults in (additive reconcile).
        let c: DeaconConfig = serde_json::from_str("{}").unwrap();
        assert!(!c.speech.enabled);
        assert_eq!(c.speech.models_dir, "tts-asr-local-models");
    }

    #[test]
    fn constitution_is_opt_in_and_parses_additively() {
        // Off by default; a config predating the section still parses.
        assert!(!DeaconConfig::default().constitution.enabled);
        let c: DeaconConfig = serde_json::from_str("{}").unwrap();
        assert!(!c.constitution.enabled);
        let c: DeaconConfig =
            serde_json::from_str(r#"{ "constitution": { "enabled": true } }"#).unwrap();
        assert!(c.constitution.enabled);
    }

    #[test]
    fn speech_round_trips() {
        let c = DeaconConfig::default();
        let json = serde_json::to_string(&c).unwrap();
        let back: DeaconConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.speech.asr.model, c.speech.asr.model);
        assert_eq!(back.speech.call.fast_model, c.speech.call.fast_model);
    }
}

#[cfg(test)]
mod providers_config_tests {
    use super::*;

    #[test]
    fn providers_default_empty_and_config_without_section_still_parses() {
        let c = DeaconConfig::default();
        assert!(c.providers.is_empty());
        assert!(c.agents_defaults.primary.is_none());
        // A config predating multi-provider still parses (additive reconcile).
        let c: DeaconConfig = serde_json::from_str("{}").unwrap();
        assert!(c.providers.is_empty());
    }

    #[test]
    fn providers_map_and_agents_defaults_round_trip() {
        let json = r#"{
            "providers": {
                "openrouter": { "kind": "openrouter", "api_key_env": "OPENROUTER_API_KEY",
                                "models": ["anthropic/claude-opus-4-8", "google/gemini-2.5-pro"] },
                "groq": { "kind": "groq", "api_key_env": "GROQ_API_KEY", "models": ["llama-3.3-70b"] }
            },
            "agents_defaults": {
                "primary": { "provider": "openrouter", "model": "anthropic/claude-opus-4-8" },
                "fallbacks": [ { "provider": "groq", "model": "llama-3.3-70b" } ]
            }
        }"#;
        let c: DeaconConfig = serde_json::from_str(json).unwrap();
        assert_eq!(c.providers.len(), 2);
        assert_eq!(c.providers["openrouter"].models.len(), 2);
        assert_eq!(c.providers["groq"].kind, ProviderKind::Groq);
        assert_eq!(
            c.agents_defaults.primary.as_ref().unwrap().provider,
            "openrouter"
        );
        assert_eq!(c.agents_defaults.fallbacks.len(), 1);
    }

    #[test]
    fn unknown_key_in_provider_spec_is_rejected() {
        let json = r#"{ "providers": { "x": { "api_key_env": "K", "modelz": [] } } }"#;
        assert!(
            serde_json::from_str::<DeaconConfig>(json).is_err(),
            "deny_unknown_fields"
        );
    }

    #[test]
    fn mom_groups_default_empty_and_round_trip() {
        assert!(DeaconConfig::default().mom.is_empty());
        let json = r#"{
            "mom": {
                "research": {
                    "proposers": ["openrouter/anthropic/claude-opus-4-8", "groq/llama-3.3-70b"],
                    "aggregator": "openrouter/google/gemini-2.5-pro",
                    "max_proposers": 2
                }
            }
        }"#;
        let c: DeaconConfig = serde_json::from_str(json).unwrap();
        let g = &c.mom["research"];
        assert_eq!(g.proposers.len(), 2);
        assert_eq!(g.aggregator, "openrouter/google/gemini-2.5-pro");
        assert_eq!(g.max_proposers, 2);
    }
}

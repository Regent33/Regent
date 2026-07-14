//! Deacon configuration schema (root). All fields have defaults so a minimal
//! (or missing) config.yaml always produces a working config; unknown keys are
//! a hard error so a typo never silently falls back to a default. The section
//! structs live in the sibling modules and are re-exported here, so every
//! consumer keeps importing them from `crate::domain::config::*`.

mod model;
mod model_lists;
mod provider_catalog;
mod provider_kind;
mod runtime;
mod speech;

pub use model::{
    AgentsDefaults, ConstitutionConfig, LimitsConfig, ModelConfig, MomGroupConfig, ProviderSpec,
};
pub use provider_catalog::OLLAMA_CLOUD_MODELS;
pub use provider_kind::{MAX_KEY_SLOTS, ProviderKind};
pub use runtime::{BoardConfig, ContextConfig, CronConfig, HttpConfig, MemoryConfig, ToolsConfig};
pub use speech::{AsrConfig, CallConfig, SpeechConfig, TtsConfig, VisionConfig, WeightFile};

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
    /// Spend/rate limits (W2.4) — currently a per-turn token ceiling.
    pub limits: LimitsConfig,
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
            limits: LimitsConfig::default(),
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

#[cfg(test)]
mod tests {
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
    fn constitution_is_on_by_default() {
        // On by default; a config predating the section inherits on. (The
        // loader additionally forces it on even if a file sets false — see
        // config_loader tests.)
        assert!(DeaconConfig::default().constitution.enabled);
        let c: DeaconConfig = serde_json::from_str("{}").unwrap();
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

    #[test]
    fn providers_default_empty_and_config_without_section_still_parses() {
        let c = DeaconConfig::default();
        assert!(c.providers.is_empty());
        assert!(c.agents_defaults.primary.is_none());
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

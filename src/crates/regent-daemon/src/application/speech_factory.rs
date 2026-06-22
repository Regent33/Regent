//! Speech provider resolution + the `voice.*` payload builders, mirroring
//! `provider_factory.rs`. The default backend is the OpenAI-compatible remote
//! adapter (OpenAI / Groq / **DashScope-Qwen**); local/command backends land
//! later. These are pure functions (config + env) so the dispatcher handlers
//! stay thin and testable; the live ASR/TTS builders that need an
//! `HttpExecutor` arrive with the reqwest wiring.

use crate::domain::config::SpeechConfig;
use regent_speech::{BUILTIN_ASR_PROVIDERS, BUILTIN_TTS_PROVIDERS};
use serde_json::{Value, json};

/// Default OpenAI-compatible base URL for a known *remote* speech provider.
/// `None` ⇒ not a remote OpenAI-compatible provider (local/command/unknown),
/// which the live builder will reject until those backends land.
#[must_use]
pub fn default_base(provider: &str) -> Option<&'static str> {
    match provider.trim().to_lowercase().as_str() {
        "groq" => Some("https://api.groq.com/openai/v1"),
        "openai" => Some("https://api.openai.com/v1"),
        "qwen" | "dashscope" => Some("https://dashscope-intl.aliyuncs.com/compatible-mode/v1"),
        _ => None,
    }
}

/// Resolve a provider's API key from the environment: a provider-specific var,
/// then generic fallbacks. Keys live in `$REGENT_HOME/.env`, loaded into the
/// process env at boot.
#[must_use]
pub fn resolve_key(provider: &str) -> String {
    let specific = match provider.trim().to_lowercase().as_str() {
        "groq" => "GROQ_API_KEY",
        "openai" => "OPENAI_API_KEY",
        "qwen" | "dashscope" => "DASHSCOPE_API_KEY",
        _ => "",
    };
    for var in [specific, "REGENT_SPEECH_API_KEY", "REGENT_API_KEY"] {
        if var.is_empty() {
            continue;
        }
        if let Ok(v) = std::env::var(var)
            && !v.trim().is_empty()
        {
            return v;
        }
    }
    String::new()
}

/// True when a provider is a supported remote backend with a usable key.
#[must_use]
pub fn provider_available(provider: &str) -> bool {
    default_base(provider).is_some() && !resolve_key(provider).is_empty()
}

/// The `voice.status` payload — pure, given resolved availability.
#[must_use]
pub fn voice_status(cfg: &SpeechConfig, asr_available: bool, tts_available: bool) -> Value {
    json!({
        "enabled": cfg.enabled,
        "models_dir": cfg.models_dir,
        "asr": { "provider": cfg.asr.provider, "model": cfg.asr.model, "available": asr_available },
        "tts": { "provider": cfg.tts.provider, "model": cfg.tts.model, "available": tts_available },
        "vision": { "input_mode": cfg.vision.input_mode },
        "call": { "fast_model": cfg.call.fast_model },
    })
}

/// The `voice.models` payload: the configured providers/models plus the
/// built-in provider names available to pick from.
#[must_use]
pub fn voice_models(cfg: &SpeechConfig) -> Value {
    json!({
        "asr": {
            "configured": { "provider": cfg.asr.provider, "model": cfg.asr.model },
            "builtins": BUILTIN_ASR_PROVIDERS,
        },
        "tts": {
            "configured": { "provider": cfg.tts.provider, "model": cfg.tts.model },
            "builtins": BUILTIN_TTS_PROVIDERS,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_remote_providers_resolve_a_base_url() {
        assert_eq!(default_base("groq"), Some("https://api.groq.com/openai/v1"));
        assert_eq!(default_base("openai"), Some("https://api.openai.com/v1"));
        assert!(default_base("QWEN").is_some()); // case-insensitive
        assert!(default_base("dashscope").is_some());
    }

    #[test]
    fn local_and_unknown_providers_have_no_remote_base() {
        assert_eq!(default_base("local"), None);
        assert_eq!(default_base("command"), None);
        assert_eq!(default_base("whatever"), None);
        // …and are therefore not "available" regardless of keys.
        assert!(!provider_available("local"));
    }

    #[test]
    fn status_payload_reflects_config_and_availability() {
        let cfg = SpeechConfig::default();
        let v = voice_status(&cfg, false, true);
        assert_eq!(v["enabled"], false);
        assert_eq!(v["asr"]["model"], "qwen3-asr");
        assert_eq!(v["asr"]["available"], false);
        assert_eq!(v["tts"]["model"], "qwen3-tts");
        assert_eq!(v["tts"]["available"], true);
        assert_eq!(v["call"]["fast_model"], "");
    }

    #[test]
    fn models_payload_lists_configured_and_builtins() {
        let v = voice_models(&SpeechConfig::default());
        assert_eq!(v["asr"]["configured"]["provider"], "local");
        let asr_builtins = v["asr"]["builtins"].as_array().unwrap();
        assert!(asr_builtins.iter().any(|p| p == "groq"));
        let tts_builtins = v["tts"]["builtins"].as_array().unwrap();
        assert!(tts_builtins.iter().any(|p| p == "edge"));
    }
}

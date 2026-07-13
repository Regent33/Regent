//! Speech provider resolution + the `voice.*` payload builders, mirroring
//! `provider_factory.rs`. The default backend is the OpenAI-compatible remote
//! adapter (OpenAI / Groq / **DashScope-Qwen**); local/command backends land
//! later. These are pure functions (config + env) so the dispatcher handlers
//! stay thin and testable; the live ASR/TTS builders that need an
//! `HttpExecutor` arrive with the reqwest wiring.

use crate::domain::config::{SpeechConfig, WeightFile};
use regent_kernel::{AsrProvider, TtsProvider};
use regent_speech::{
    BUILTIN_ASR_PROVIDERS, BUILTIN_TTS_PROVIDERS, HttpExecutor, ModelFile, ModelKind, ModelSpec,
    OpenAiCompatAsr, OpenAiCompatTts,
};
use serde_json::{Value, json};
use std::sync::Arc;

/// Resolve the OpenAI-compatible base URL for a provider, honoring an explicit
/// `override_url`. **`local` is the default** — Qwen3 served by a localhost
/// server (the same shape this repo uses for Ollama). `None` ⇒ unknown provider
/// (command/native land later), which the live builder rejects.
#[must_use]
pub fn resolve_base(provider: &str, override_url: &str) -> Option<String> {
    let trimmed = override_url.trim();
    if !trimmed.is_empty() {
        return Some(trimmed.to_owned());
    }
    let url = match provider.trim().to_lowercase().as_str() {
        "local" => "http://localhost:8000/v1", // e.g. a local vLLM serving Qwen3 speech
        "groq" => "https://api.groq.com/openai/v1",
        "openai" => "https://api.openai.com/v1",
        "qwen" | "dashscope" => "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
        _ => return None,
    };
    Some(url.to_owned())
}

/// Whether a provider needs an API key. `local` does not (localhost server).
#[must_use]
pub fn needs_key(provider: &str) -> bool {
    !matches!(provider.trim().to_lowercase().as_str(), "local")
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

/// True when a provider is configured enough to use: a known base URL, and a key
/// if it needs one. `local` is available as soon as it's configured (reachability
/// of the localhost server is proven by `voice.test`, not here).
#[must_use]
pub fn provider_available(provider: &str, base_override: &str) -> bool {
    resolve_base(provider, base_override).is_some()
        && (!needs_key(provider) || !resolve_key(provider).is_empty())
}

/// Build the configured ASR provider, or an error naming what to fix. Only
/// remote OpenAI-compatible backends are wired today (local/command land later).
pub fn make_asr<E: HttpExecutor + ?Sized + 'static>(
    cfg: &SpeechConfig,
    exec: Arc<E>,
) -> Result<Arc<dyn AsrProvider>, String> {
    let provider = cfg.asr.provider.trim().to_lowercase();
    let Some(base) = resolve_base(&provider, &cfg.asr.base_url) else {
        return Err(unsupported(&provider, "asr"));
    };
    Ok(Arc::new(OpenAiCompatAsr::new(
        provider.clone(),
        base,
        resolve_key(&provider),
        cfg.asr.model.clone(),
        exec,
    )))
}

/// Build the configured TTS provider, or an error naming what to fix.
pub fn make_tts<E: HttpExecutor + ?Sized + 'static>(
    cfg: &SpeechConfig,
    exec: Arc<E>,
) -> Result<Arc<dyn TtsProvider>, String> {
    let provider = cfg.tts.provider.trim().to_lowercase();
    let Some(base) = resolve_base(&provider, &cfg.tts.base_url) else {
        return Err(unsupported(&provider, "tts"));
    };
    Ok(Arc::new(OpenAiCompatTts::new(
        provider.clone(),
        base,
        resolve_key(&provider),
        cfg.tts.model.clone(),
        exec,
    )))
}

fn unsupported(provider: &str, kind: &str) -> String {
    format!(
        "{kind} provider '{provider}' is not wired yet — use 'local' (a localhost \
         Qwen3 server) or a remote provider (qwen, groq, openai) via \
         `regent voice setup`, or configure a command provider"
    )
}

/// The `voice.status` payload — pure given resolved availability, except for
/// `whisper_size`: `REGENT_WHISPER_SIZE` lives in `.env`, not `SpeechConfig`
/// (see `voice_set_ops::voice_set`), and `upsert_env_var` hot-applies it to
/// this process, so reading it here reflects a `voice.set` immediately —
/// matching `resolve_key`'s direct `std::env::var` use elsewhere in this file.
/// Defaults to `"small"`, mirroring `regent-voice-server`'s own fallback, so
/// the picker always shows the size actually in effect.
#[must_use]
pub fn voice_status(cfg: &SpeechConfig, asr_available: bool, tts_available: bool) -> Value {
    json!({
        "enabled": cfg.enabled,
        "models_dir": cfg.models_dir,
        "asr": { "provider": cfg.asr.provider, "model": cfg.asr.model, "available": asr_available },
        "tts": { "provider": cfg.tts.provider, "model": cfg.tts.model, "available": tts_available },
        "vision": { "input_mode": cfg.vision.input_mode },
        "call": { "fast_model": cfg.call.fast_model },
        "whisper_size": std::env::var("REGENT_WHISPER_SIZE").unwrap_or_else(|_| "small".into()),
        // Local call TTS voice + rate — same .env-backed pattern as
        // whisper_size; "0"/"1" mirror KokoroEngine's own defaults.
        "kokoro_speaker": std::env::var("REGENT_KOKORO_SPEAKER").unwrap_or_else(|_| "0".into()),
        "kokoro_speed": std::env::var("REGENT_KOKORO_SPEED").unwrap_or_else(|_| "1".into()),
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

/// Build the model-download specs for the configured local weights — one spec
/// per kind that has `weights` set. Empty when nothing is configured to download
/// (a hosted provider, or a localhost server you run yourself). The spec id is
/// the configured model name, so files cache under `<models_dir>/<kind>/<model>`.
#[must_use]
pub fn weight_specs(cfg: &SpeechConfig) -> Vec<ModelSpec> {
    let mut specs = Vec::new();
    if !cfg.asr.weights.is_empty() {
        specs.push(ModelSpec {
            kind: ModelKind::Asr,
            id: cfg.asr.model.clone(),
            files: cfg.asr.weights.iter().map(to_model_file).collect(),
        });
    }
    if !cfg.tts.weights.is_empty() {
        specs.push(ModelSpec {
            kind: ModelKind::Tts,
            id: cfg.tts.model.clone(),
            files: cfg.tts.weights.iter().map(to_model_file).collect(),
        });
    }
    specs
}

fn to_model_file(w: &WeightFile) -> ModelFile {
    ModelFile {
        name: w.name.clone(),
        url: w.url.clone(),
        sha256: w.sha256.clone(),
    }
}

#[cfg(test)]
#[path = "speech_factory_tests.rs"]
mod tests;

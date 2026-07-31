//! Speech provider resolution + the `voice.*` payload builders, mirroring
//! `provider_factory.rs`. The default backend is the OpenAI-compatible remote
//! adapter (OpenAI / Groq / **DashScope-Qwen**). Butler's native local call
//! server is Whisper + Kokoro; these compatibility builders serve the other
//! voice surfaces. These are pure functions (config + env) so the dispatcher handlers
//! stay thin and testable; the live ASR/TTS builders that need an
//! `HttpExecutor` arrive with the reqwest wiring.

use crate::domain::config::{SpeechConfig, WeightFile};
use regent_kernel::{AsrProvider, TtsProvider};
use regent_speech::{
    HttpExecutor, ModelFile, ModelKind, ModelSpec, OpenAiCompatAsr, OpenAiCompatTts, catalog,
};
use serde_json::{Value, json};
use std::sync::Arc;

/// Resolve the OpenAI-compatible base URL for a provider, honoring an explicit
/// `override_url`. Known ids come from [`regent_speech::catalog`]; `None` ⇒ an
/// unknown id with no override, which the live builder rejects with a message
/// naming the fix.
///
/// The override is checked FIRST and short-circuits the table on purpose: it is
/// how an unlisted host, a moved self-hosted port, or Azure's per-deployment URL
/// reaches the adapter without waiting for a release.
#[must_use]
pub fn resolve_base(provider: &str, override_url: &str) -> Option<String> {
    let trimmed = override_url.trim();
    if !trimmed.is_empty() {
        return Some(trimmed.to_owned());
    }
    // An empty catalog base means "no fixed host" (Azure/RunPod/custom) — treat
    // it as unresolved so the caller's error tells the user to set base_url.
    catalog::find(provider)
        .filter(|p| !p.base_url.is_empty())
        .map(|p| p.base_url.to_owned())
}

/// Whether a provider needs an API key — true for hosted APIs, false for a
/// server you run yourself. Unknown ids are assumed to need one (they are
/// reached via `base_url`, and a hosted endpoint is the safer assumption than
/// silently sending an unauthenticated request).
#[must_use]
pub fn needs_key(provider: &str) -> bool {
    catalog::find(provider).is_none_or(|p| p.key_var.is_some())
}

/// Resolve a provider's API key from the environment: the catalog's
/// provider-specific var, then generic fallbacks (which are also how an
/// unlisted `base_url` provider gets its key). Keys live in
/// `$REGENT_HOME/.env`, loaded into the process env at boot.
#[must_use]
pub fn resolve_key(provider: &str) -> String {
    let specific = catalog::find(provider)
        .and_then(|p| p.key_var)
        .unwrap_or("");
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

/// The configured model for a kind, falling back to the catalog's default when
/// the config leaves it empty — so switching provider without also editing the
/// model id lands on something that actually exists on that endpoint.
fn model_for(configured: &str, provider: &str, kind: ModelKindHint) -> String {
    let configured = configured.trim();
    if !configured.is_empty() {
        return configured.to_owned();
    }
    catalog::find(provider)
        .and_then(|p| match kind {
            ModelKindHint::Asr => p.asr_model,
            ModelKindHint::Tts => p.tts_model,
        })
        .unwrap_or_default()
        .to_owned()
}

#[derive(Clone, Copy)]
enum ModelKindHint {
    Asr,
    Tts,
}

/// True when a provider is configured enough to use: a known base URL, and a key
/// if it needs one. `local` is available as soon as it's configured (reachability
/// of the localhost server is proven by `voice.test`, not here).
#[must_use]
pub fn provider_available(provider: &str, base_override: &str) -> bool {
    resolve_base(provider, base_override).is_some()
        && (!needs_key(provider) || !resolve_key(provider).is_empty())
}

/// Build the configured ASR provider, or an error naming what to fix.
pub fn make_asr<E: HttpExecutor + ?Sized + 'static>(
    cfg: &SpeechConfig,
    exec: Arc<E>,
) -> Result<Arc<dyn AsrProvider>, String> {
    let provider = cfg.asr.provider.trim().to_lowercase();
    let Some(base) = resolve_base(&provider, &cfg.asr.base_url) else {
        return Err(unsupported(&provider, "asr", &catalog::asr_ids()));
    };
    // A known provider that only does TTS is a config mistake worth naming —
    // building the adapter would just 404 at call time.
    if let Some(p) = catalog::find(&provider)
        && p.asr_model.is_none()
        && cfg.asr.base_url.trim().is_empty()
    {
        return Err(wrong_kind(&provider, "speech-to-text", &catalog::asr_ids()));
    }
    Ok(Arc::new(OpenAiCompatAsr::new(
        provider.clone(),
        base,
        resolve_key(&provider),
        model_for(&cfg.asr.model, &provider, ModelKindHint::Asr),
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
        return Err(unsupported(&provider, "tts", &catalog::tts_ids()));
    };
    if let Some(p) = catalog::find(&provider)
        && p.tts_model.is_none()
        && cfg.tts.base_url.trim().is_empty()
    {
        return Err(wrong_kind(&provider, "text-to-speech", &catalog::tts_ids()));
    }
    Ok(Arc::new(OpenAiCompatTts::new(
        provider.clone(),
        base,
        resolve_key(&provider),
        model_for(&cfg.tts.model, &provider, ModelKindHint::Tts),
        exec,
    )))
}

fn unsupported(provider: &str, kind: &str, known: &[&str]) -> String {
    format!(
        "{kind} provider '{provider}' is not known — pick one of: {} \
         (`regent voice setup`), or set `speech.{kind}.base_url` to any \
         OpenAI-compatible endpoint",
        known.join(", ")
    )
}

fn wrong_kind(provider: &str, kind: &str, known: &[&str]) -> String {
    format!(
        "provider '{provider}' does not do {kind} — pick one of: {} \
         (or set a base_url if your deployment does)",
        known.join(", ")
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
        // Empty = auto-detect language (the picker maps "" → "Auto").
        "whisper_lang": std::env::var("REGENT_WHISPER_LANG").unwrap_or_default(),
        // Local call TTS voice + rate — same .env-backed pattern as
        // whisper_size; "0"/"1" mirror KokoroEngine's own defaults.
        "kokoro_speaker": std::env::var("REGENT_KOKORO_SPEAKER").unwrap_or_else(|_| "0".into()),
        "kokoro_speed": std::env::var("REGENT_KOKORO_SPEED").unwrap_or_else(|_| "1".into()),
    })
}

/// The `voice.models` payload: the configured providers/models, the ids that
/// can serve each kind, and (additive) the full catalog so a picker can show
/// labels, blurbs, key vars and defaults without hardcoding a second copy of
/// the table. Every listed id resolves — the lists are derived from the same
/// catalog the factory dispatches on.
#[must_use]
pub fn voice_models(cfg: &SpeechConfig) -> Value {
    let catalog: Vec<Value> = catalog::SPEECH_PROVIDERS
        .iter()
        .map(|p| {
            json!({
                "id": p.id,
                "label": p.label,
                "blurb": p.blurb,
                "base_url": p.base_url,
                "key_var": p.key_var,
                "asr_model": p.asr_model,
                "tts_model": p.tts_model,
                "hosted": p.hosted,
            })
        })
        .collect();
    json!({
        "asr": {
            "configured": { "provider": cfg.asr.provider, "model": cfg.asr.model },
            "builtins": catalog::asr_ids(),
        },
        "tts": {
            "configured": { "provider": cfg.tts.provider, "model": cfg.tts.model },
            "builtins": catalog::tts_ids(),
        },
        "catalog": catalog,
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

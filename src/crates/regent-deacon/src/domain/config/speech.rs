//! Voice/vision stack config. **Off by default** — a fresh deacon loads or
//! downloads no speech model until `regent voice setup` flips `enabled`.
//! Defaults are Qwen3-ASR/Qwen3-TTS, swappable to any provider/model.

use serde::{Deserialize, Serialize};

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

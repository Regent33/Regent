//! Speech data types (buffers, options, catalog rows). Split from
//! `speech.rs` (file-size rule); re-exported there so paths are stable.

use crate::types::error::RegentError;
use serde::{Deserialize, Serialize};

/// Decoded PCM audio. Container codecs (ogg/opus/mp3/m4a/webm) are decoded to
/// PCM at the edge (ffmpeg) before reaching a provider; capture is 16 kHz mono
/// `i16` (Whisper-native, from Hermes `voice_mode.py`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioBuffer {
    pub samples: Vec<i16>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl AudioBuffer {
    #[must_use]
    pub fn new(samples: Vec<i16>, sample_rate: u32, channels: u16) -> Self {
        Self {
            samples,
            sample_rate,
            channels,
        }
    }

    /// Duration in milliseconds. `0` when `sample_rate` or `channels` is `0`
    /// (an empty/invalid buffer) rather than dividing by zero.
    #[must_use]
    pub fn duration_ms(&self) -> u64 {
        let frames_per_channel = self.sample_rate as u64 * self.channels.max(1) as u64;
        if frames_per_channel == 0 {
            return 0;
        }
        (self.samples.len() as u64 * 1000) / frames_per_channel
    }
}

/// Encoded audio bytes plus their container format — what a TTS provider
/// returns (providers emit encoded audio directly; re-decoding to PCM would be
/// wasteful and lossy). The gateway converts to Opus for a voice bubble when
/// the provider is not [`TtsProvider::voice_compatible`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SynthesizedAudio {
    pub bytes: Vec<u8>,
    pub format: AudioFormat,
}

/// Audio container/encoding. `lowercase` on the wire to match config
/// (`tts.format: opus`) and Hermes's format set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    #[default]
    Mp3,
    Wav,
    Ogg,
    Opus,
    Flac,
}

impl AudioFormat {
    /// File extension (no dot) for this format.
    #[must_use]
    pub fn ext(self) -> &'static str {
        match self {
            Self::Mp3 => "mp3",
            Self::Wav => "wav",
            Self::Ogg => "ogg",
            Self::Opus => "opus",
            Self::Flac => "flac",
        }
    }
}

/// A transcription result. `text` is empty (not an error) when the input was
/// silence or a filtered hallucination — callers treat `""` as "nothing said".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transcription {
    pub text: String,
    /// Detected/echoed language (BCP-47), if the provider reports one.
    pub language: Option<String>,
    /// Provider name that produced this, for diagnostics.
    pub provider: String,
}

/// Per-call ASR knobs. All optional — a provider ignores what it can't honor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AsrOptions {
    pub model: Option<String>,
    /// BCP-47 language hint (e.g. `"en"`); `None`/`"auto"` lets the model detect.
    pub language: Option<String>,
}

/// Per-call TTS knobs. `Default` gives no voice/model/speed and the default
/// format (`AudioFormat::Mp3`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TtsOptions {
    pub voice: Option<String>,
    pub model: Option<String>,
    /// Speech-rate multiplier (`1.0` = normal). Providers without rate control ignore it.
    pub speed: Option<f32>,
    pub format: AudioFormat,
}

/// One entry in a provider's model catalog (powers `regent voice models`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub display: Option<String>,
    #[serde(default)]
    pub languages: Vec<String>,
}

/// One TTS voice (powers `regent voice voices`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceInfo {
    pub id: String,
    pub display: Option<String>,
    pub language: Option<String>,
}

/// One credential a provider needs, with where to obtain it — rendered as a
/// prompt by the `voice setup` wizard (Hermes's `get_setup_schema().env_vars`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvVarPrompt {
    pub key: String,
    pub prompt: String,
    pub url: Option<String>,
}

/// What a provider needs configured — consumed by the `voice setup` wizard and
/// `voice status`. Empty default = no setup required (e.g. a free local model).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSetup {
    pub display_name: String,
    /// Short tag, e.g. `"free"` | `"paid"` | `"local"`.
    pub badge: String,
    #[serde(default)]
    pub env_vars: Vec<EnvVarPrompt>,
}

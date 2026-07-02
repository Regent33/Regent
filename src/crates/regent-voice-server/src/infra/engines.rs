//! ASR/TTS engine ports. The server logic depends only on these traits; the
//! local ONNX implementations (sherpa: whisper + piper/kokoro) arrive behind
//! a cargo feature in the next slice — until then `Engines::from_env()`
//! reports why speech is unavailable instead of failing silently.

use regent_kernel::AudioBuffer;
use std::sync::Arc;

/// Speech → text. Blocking (called via `spawn_blocking`).
pub trait AsrEngine: Send + Sync {
    /// Transcribe an audio container's bytes (WAV from the call UI; OGG/etc.
    /// from platform voice notes). `language` is a hint, `None` = auto.
    fn transcribe(&self, audio: &[u8], language: Option<&str>) -> Result<String, String>;
}

/// Text → speech. Blocking (called via `spawn_blocking`).
pub trait TtsEngine: Send + Sync {
    fn synthesize(&self, text: &str) -> Result<AudioBuffer, String>;
}

/// The loaded engine pair — either may be absent, with the reason kept for
/// `/health` and the console (never a silent fallback).
#[derive(Clone, Default)]
pub struct Engines {
    pub asr: Option<Arc<dyn AsrEngine>>,
    pub tts: Option<Arc<dyn TtsEngine>>,
    /// Why an engine is missing (shown in /health and at startup).
    pub note: String,
}

impl Engines {
    /// Loads whatever local engines this build carries. The base build has
    /// none — the OpenAI-compatible endpoints answer 503 with this note.
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            asr: None,
            tts: None,
            note: "local ASR/TTS engines not built into this binary yet — \
                   run the python voice server for speech, or rebuild with the \
                   local engine feature (next slice)"
                .into(),
        }
    }

    #[must_use]
    pub fn ready(&self) -> bool {
        self.asr.is_some() && self.tts.is_some()
    }
}

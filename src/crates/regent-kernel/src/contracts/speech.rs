//! Speech contracts — ASR (speech→text) and TTS (text→speech): the pluggable
//! voice stack. Sync + swappable at the composition root, exactly like
//! [`EmbeddingProvider`](crate::EmbeddingProvider); concrete backends (local
//! Qwen3, remote Whisper/ElevenLabs, a shell `command` provider) live in
//! `regent-speech`. The traits carry more than one method on purpose —
//! modeled on Hermes's `TranscriptionProvider`/`TTSProvider` ABCs, because the
//! `voice setup` wizard (`setup_schema`), `voice status` (`is_available`),
//! `voice models/voices` (`list_*`), and the gateway voice-bubble path
//! (`voice_compatible`) all read the extras. Streaming uses a callback sink
//! like [`DeltaSink`](crate) rather than a `Stream`, so the kernel stays free
//! of async/futures deps.

use crate::types::error::RegentError;

pub use super::speech_types::{
    AsrOptions, AudioBuffer, AudioFormat, EnvVarPrompt, ModelInfo, ProviderSetup, SynthesizedAudio,
    Transcription, TtsOptions, VoiceInfo,
};

/// A callback receiving encoded-audio fragments as they are produced. Mirrors
/// `providers::DeltaSink` — the kernel's streaming idiom, no `Stream` needed.
pub type AudioSink<'a> = &'a (dyn Fn(&[u8]) + Send + Sync);

/// Speech-to-text backend. Sync and swappable, like [`EmbeddingProvider`];
/// network-bound implementations are driven from a `spawn_blocking` context by
/// the deacon/gateway. Implementations must **not** panic — map failures to
/// [`RegentError`].
///
/// [`EmbeddingProvider`]: crate::EmbeddingProvider
pub trait AsrProvider: Send + Sync {
    /// Stable lowercase id used in `asr.provider` config.
    fn name(&self) -> &str;

    /// Transcribe decoded PCM. Returns an empty `text` for silence rather than
    /// an error (the robustness layer filters hallucinations to `""`).
    fn transcribe(
        &self,
        audio: &AudioBuffer,
        opts: &AsrOptions,
    ) -> Result<Transcription, RegentError>;

    /// Transcribe an already-encoded audio file by its raw bytes (e.g. a Telegram
    /// OGG/Opus voice note), skipping PCM decode. Whisper-style endpoints accept
    /// ogg/mp3/m4a/wav directly, so the HTTP backends override this; the default
    /// errors so a PCM-only engine opts in explicitly rather than silently
    /// mis-transcribing. `filename`'s extension is how the endpoint sniffs format.
    fn transcribe_file(
        &self,
        _bytes: &[u8],
        _filename: &str,
        _opts: &AsrOptions,
    ) -> Result<Transcription, RegentError> {
        Err(RegentError::Provider(format!(
            "{} cannot transcribe an encoded file; decode to PCM and use transcribe()",
            self.name()
        )))
    }

    /// True when this provider can service calls (key present, model installed).
    /// Drives `voice status` and the setup picker; must not panic.
    fn is_available(&self) -> bool {
        true
    }

    /// Model catalog, or empty when the provider has one fixed model.
    fn list_models(&self) -> Vec<ModelInfo> {
        Vec::new()
    }

    /// Setup metadata (keys + where to get them) for the `voice setup` wizard.
    fn setup_schema(&self) -> ProviderSetup {
        ProviderSetup::default()
    }
}

/// Text-to-speech backend. See [`AsrProvider`] for the sync/no-panic contract.
pub trait TtsProvider: Send + Sync {
    /// Stable lowercase id used in `tts.provider` config.
    fn name(&self) -> &str;

    /// Synthesize the whole utterance.
    fn synthesize(&self, text: &str, opts: &TtsOptions) -> Result<SynthesizedAudio, RegentError>;

    /// Streaming synthesis: invoke `on_chunk` for each fragment as it is
    /// produced, returning the fully-accumulated audio. The default synthesizes
    /// fully then emits once, so providers without a streaming path still
    /// satisfy the contract (mirrors `ChatProvider::complete_streaming`).
    fn synthesize_streaming(
        &self,
        text: &str,
        opts: &TtsOptions,
        on_chunk: AudioSink<'_>,
    ) -> Result<SynthesizedAudio, RegentError> {
        let audio = self.synthesize(text, opts)?;
        if !audio.bytes.is_empty() {
            on_chunk(&audio.bytes);
        }
        Ok(audio)
    }

    /// Whether output is fit for a voice bubble (Opus). When `false`, the
    /// gateway runs ffmpeg to convert before `sendVoice`. Opt-in (default `false`).
    fn voice_compatible(&self) -> bool {
        false
    }

    fn is_available(&self) -> bool {
        true
    }

    /// Voice catalog, or empty when the provider exposes none.
    fn list_voices(&self) -> Vec<VoiceInfo> {
        Vec::new()
    }

    fn setup_schema(&self) -> ProviderSetup {
        ProviderSetup::default()
    }
}

#[cfg(test)]
#[path = "speech_tests.rs"]
mod tests;

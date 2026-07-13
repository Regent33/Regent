//! OpenAI-compatible speech-to-text provider.

use super::{HttpExecutor, build_transcription_request, parse_transcription_response};
use crate::wav;
use regent_kernel::{
    AsrOptions, AsrProvider, AudioBuffer, ProviderSetup, RegentError, Transcription,
};
use std::sync::Arc;

/// OpenAI-compatible speech-to-text. Construct one per endpoint (OpenAI, Groq,
/// DashScope/Qwen) with the matching `base_url`, `api_key`, and `model`.
pub struct OpenAiCompatAsr<E: HttpExecutor + ?Sized> {
    name: String,
    base_url: String,
    api_key: String,
    model: String,
    exec: Arc<E>,
}

impl<E: HttpExecutor + ?Sized> OpenAiCompatAsr<E> {
    pub fn new(
        name: impl Into<String>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
        exec: Arc<E>,
    ) -> Self {
        Self {
            name: name.into(),
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: model.into(),
            exec,
        }
    }
}

impl<E: HttpExecutor + ?Sized> AsrProvider for OpenAiCompatAsr<E> {
    fn name(&self) -> &str {
        &self.name
    }

    fn transcribe(
        &self,
        audio: &AudioBuffer,
        opts: &AsrOptions,
    ) -> Result<Transcription, RegentError> {
        let model = opts.model.as_deref().unwrap_or(&self.model);
        let req = build_transcription_request(
            &self.base_url,
            &self.api_key,
            model,
            "audio.wav",
            wav::encode(audio),
        );
        let bytes = self
            .exec
            .execute(req)
            .map_err(|e| RegentError::Provider(format!("{} ASR: {e}", self.name)))?;
        Ok(Transcription {
            text: parse_transcription_response(&bytes),
            language: opts.language.clone(),
            provider: self.name.clone(),
        })
    }

    /// Pass encoded audio (e.g. a Telegram `voice.ogg`) straight to the endpoint
    /// — Whisper-style APIs accept ogg/mp3/m4a/wav, so no PCM decode is needed.
    fn transcribe_file(
        &self,
        bytes: &[u8],
        filename: &str,
        opts: &AsrOptions,
    ) -> Result<Transcription, RegentError> {
        let model = opts.model.as_deref().unwrap_or(&self.model);
        let req = build_transcription_request(
            &self.base_url,
            &self.api_key,
            model,
            filename,
            bytes.to_vec(),
        );
        let out = self
            .exec
            .execute(req)
            .map_err(|e| RegentError::Provider(format!("{} ASR: {e}", self.name)))?;
        Ok(Transcription {
            text: parse_transcription_response(&out),
            language: opts.language.clone(),
            provider: self.name.clone(),
        })
    }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    fn setup_schema(&self) -> ProviderSetup {
        ProviderSetup {
            display_name: self.name.clone(),
            badge: "api".to_owned(),
            env_vars: Vec::new(),
        }
    }
}

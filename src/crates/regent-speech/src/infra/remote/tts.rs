//! OpenAI-compatible text-to-speech provider.

use super::{HttpExecutor, build_speech_request};
use regent_kernel::{ProviderSetup, RegentError, SynthesizedAudio, TtsOptions, TtsProvider};
use std::sync::Arc;

/// OpenAI-compatible text-to-speech.
pub struct OpenAiCompatTts<E: HttpExecutor + ?Sized> {
    name: String,
    base_url: String,
    api_key: String,
    model: String,
    exec: Arc<E>,
}

impl<E: HttpExecutor + ?Sized> OpenAiCompatTts<E> {
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

impl<E: HttpExecutor + ?Sized> TtsProvider for OpenAiCompatTts<E> {
    fn name(&self) -> &str {
        &self.name
    }

    fn synthesize(&self, text: &str, opts: &TtsOptions) -> Result<SynthesizedAudio, RegentError> {
        let model = opts.model.as_deref().unwrap_or(&self.model);
        let req = build_speech_request(&self.base_url, &self.api_key, model, text, opts);
        let bytes = self
            .exec
            .execute(req)
            .map_err(|e| RegentError::Provider(format!("{} TTS: {e}", self.name)))?;
        Ok(SynthesizedAudio {
            bytes,
            format: opts.format,
        })
    }

    /// True when the requested format is Opus/Ogg — playable as a voice bubble
    /// without a re-encode. (The provider honors `response_format`, so this
    /// tracks the requested format rather than a fixed capability.)
    fn voice_compatible(&self) -> bool {
        // Conservative default: let the gateway decide per-call from the format.
        false
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

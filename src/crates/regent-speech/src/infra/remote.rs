//! OpenAI-compatible remote ASR/TTS — one adapter, many endpoints.
//!
//! Grounded in how Hermes does it: its Groq and OpenAI STT backends are the
//! *same* wire (`{base}/audio/transcriptions`, multipart `file` + `model`),
//! differing only by `base_url` + key; OpenAI TTS is `{base}/audio/speech`
//! (JSON `model`/`voice`/`input`/`response_format`). So a single
//! OpenAI-compatible adapter covers OpenAI, Groq, **and Alibaba DashScope's
//! compatible mode** — which is how Regent's default `qwen3-asr` / `qwen3-tts`
//! are served (no local inference required). This mirrors `regent-providers`'
//! one-OpenAI-compatible-chat-adapter-many-base-URLs design.
//!
//! The HTTP call is **injected** ([`HttpExecutor`]) so this crate stays
//! network-free and fully unit-testable; the daemon supplies a reqwest-backed
//! executor. Request building and response parsing are pure functions.

use crate::wav;
use regent_kernel::{
    AsrOptions, AsrProvider, AudioBuffer, ProviderSetup, RegentError, SynthesizedAudio,
    Transcription, TtsOptions, TtsProvider,
};
use std::sync::Arc;

/// A built HTTP request for a speech endpoint. The executor turns this into a
/// real call; tests assert on it directly.
#[derive(Debug, Clone, PartialEq)]
pub struct SpeechHttpRequest {
    pub url: String,
    /// Bearer token for `Authorization`. Empty ⇒ the caller is unauthenticated
    /// (the provider's `is_available` should have caught this first).
    pub api_key: String,
    pub body: HttpBody,
}

/// Request payload: JSON (TTS) or multipart with one audio file (ASR).
#[derive(Debug, Clone, PartialEq)]
pub enum HttpBody {
    Json(serde_json::Value),
    Multipart {
        /// Plain text form fields (e.g. `model`, `response_format`).
        fields: Vec<(String, String)>,
        /// The audio part: `(field_name, filename, bytes)`.
        file: (String, String, Vec<u8>),
    },
}

/// Executes a built [`SpeechHttpRequest`], returning the raw response body.
/// Implemented by the daemon over reqwest; mocked in tests.
pub trait HttpExecutor: Send + Sync {
    fn execute(&self, request: SpeechHttpRequest) -> Result<Vec<u8>, String>;
}

/// Build the multipart transcription request (`{base}/audio/transcriptions`).
/// `filename`'s extension tells the endpoint the format (`audio.wav` for PCM we
/// encoded, `voice.ogg` for a Telegram voice note passed through untouched).
#[must_use]
pub fn build_transcription_request(
    base_url: &str,
    api_key: &str,
    model: &str,
    filename: &str,
    audio: Vec<u8>,
) -> SpeechHttpRequest {
    SpeechHttpRequest {
        url: format!("{}/audio/transcriptions", base_url.trim_end_matches('/')),
        api_key: api_key.to_owned(),
        body: HttpBody::Multipart {
            fields: vec![
                ("model".to_owned(), model.to_owned()),
                // Plain text out — simplest to parse; matches Hermes for whisper-1.
                ("response_format".to_owned(), "text".to_owned()),
            ],
            file: ("file".to_owned(), filename.to_owned(), audio),
        },
    }
}

/// Build the JSON speech request (`{base}/audio/speech`).
#[must_use]
pub fn build_speech_request(
    base_url: &str,
    api_key: &str,
    model: &str,
    text: &str,
    opts: &TtsOptions,
) -> SpeechHttpRequest {
    let mut body = serde_json::json!({
        "model": model,
        "input": text,
        "voice": opts.voice.as_deref().unwrap_or("alloy"),
        "response_format": opts.format.ext(),
    });
    if let Some(speed) = opts.speed {
        // Clamp like Hermes (0.25..=4.0).
        body["speed"] = serde_json::json!(speed.clamp(0.25, 4.0));
    }
    SpeechHttpRequest {
        url: format!("{}/audio/speech", base_url.trim_end_matches('/')),
        api_key: api_key.to_owned(),
        body: HttpBody::Json(body),
    }
}

/// Parse a transcription response body. With `response_format=text` the body is
/// the transcript; some endpoints return JSON `{"text": "..."}` regardless, so
/// try that first.
#[must_use]
pub fn parse_transcription_response(bytes: &[u8]) -> String {
    let raw = String::from_utf8_lossy(bytes);
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw)
        && let Some(text) = v.get("text").and_then(serde_json::Value::as_str)
    {
        return text.trim().to_owned();
    }
    raw.trim().to_owned()
}

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
        let req =
            build_transcription_request(&self.base_url, &self.api_key, model, filename, bytes.to_vec());
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

    fn synthesize(
        &self,
        text: &str,
        opts: &TtsOptions,
    ) -> Result<SynthesizedAudio, RegentError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use regent_kernel::AudioFormat;
    use std::sync::Mutex;

    /// Records the last request and returns canned bytes.
    struct MockExecutor {
        response: Vec<u8>,
        seen: Mutex<Option<SpeechHttpRequest>>,
    }
    impl MockExecutor {
        fn new(response: impl Into<Vec<u8>>) -> Arc<Self> {
            Arc::new(Self {
                response: response.into(),
                seen: Mutex::new(None),
            })
        }
    }
    impl HttpExecutor for MockExecutor {
        fn execute(&self, request: SpeechHttpRequest) -> Result<Vec<u8>, String> {
            *self.seen.lock().unwrap() = Some(request);
            Ok(self.response.clone())
        }
    }

    #[test]
    fn transcription_request_is_multipart_to_the_right_url() {
        let req = build_transcription_request(
            "https://api.groq.com/openai/v1/",
            "k",
            "whisper-large-v3-turbo",
            "audio.wav",
            vec![1, 2, 3],
        );
        assert_eq!(req.url, "https://api.groq.com/openai/v1/audio/transcriptions");
        match req.body {
            HttpBody::Multipart { fields, file } => {
                assert!(fields.contains(&("model".into(), "whisper-large-v3-turbo".into())));
                assert_eq!(file.0, "file");
                assert_eq!(file.1, "audio.wav");
                assert_eq!(file.2, vec![1, 2, 3]);
            }
            HttpBody::Json(_) => panic!("expected multipart"),
        }
    }

    #[test]
    fn transcribe_file_sends_raw_bytes_under_the_given_filename() {
        let exec = MockExecutor::new("hello from a voice note");
        let asr = OpenAiCompatAsr::new("groq", "https://api.groq.com/openai/v1", "k", "whisper-1", Arc::clone(&exec));
        let out = asr
            .transcribe_file(&[0xAA, 0xBB], "voice.ogg", &AsrOptions::default())
            .unwrap();
        assert_eq!(out.text, "hello from a voice note");
        let seen = exec.seen.lock().unwrap().clone().unwrap();
        let HttpBody::Multipart { file, .. } = seen.body else {
            panic!("expected multipart");
        };
        // Raw OGG passed through untouched (no WAV re-encode) under voice.ogg.
        assert_eq!(file.1, "voice.ogg");
        assert_eq!(file.2, vec![0xAA, 0xBB]);
    }

    #[test]
    fn speech_request_carries_model_voice_and_format() {
        let opts = TtsOptions {
            voice: Some("nova".into()),
            format: AudioFormat::Opus,
            speed: Some(9.0), // out of range → clamped
            ..TtsOptions::default()
        };
        let req = build_speech_request("https://x/v1", "k", "qwen3-tts", "hi", &opts);
        assert_eq!(req.url, "https://x/v1/audio/speech");
        let HttpBody::Json(body) = req.body else {
            panic!("expected json");
        };
        assert_eq!(body["model"], "qwen3-tts");
        assert_eq!(body["voice"], "nova");
        assert_eq!(body["input"], "hi");
        assert_eq!(body["response_format"], "opus");
        assert_eq!(body["speed"], 4.0);
    }

    #[test]
    fn parse_transcription_handles_text_and_json() {
        assert_eq!(parse_transcription_response(b"  hello world \n"), "hello world");
        assert_eq!(
            parse_transcription_response(br#"{"text": "from json"}"#),
            "from json"
        );
    }

    #[test]
    fn asr_provider_round_trips_through_executor() {
        let exec = MockExecutor::new("transcribed text");
        let asr = OpenAiCompatAsr::new(
            "qwen",
            "https://dashscope/compatible-mode/v1",
            "secret",
            "qwen3-asr",
            Arc::clone(&exec),
        );
        assert!(asr.is_available());
        let audio = AudioBuffer::new(vec![0; 160], 16_000, 1);
        let out = asr.transcribe(&audio, &AsrOptions::default()).unwrap();
        assert_eq!(out.text, "transcribed text");
        assert_eq!(out.provider, "qwen");
        // The executor saw a transcription request with a WAV payload.
        let seen = exec.seen.lock().unwrap().clone().unwrap();
        assert!(seen.url.ends_with("/audio/transcriptions"));
    }

    #[test]
    fn tts_provider_returns_bytes_in_requested_format() {
        let exec = MockExecutor::new(vec![9, 9, 9]);
        let tts = OpenAiCompatTts::new("qwen", "https://x/v1", "secret", "qwen3-tts", exec);
        let opts = TtsOptions {
            format: AudioFormat::Opus,
            ..TtsOptions::default()
        };
        let out = tts.synthesize("hello", &opts).unwrap();
        assert_eq!(out.bytes, vec![9, 9, 9]);
        assert_eq!(out.format, AudioFormat::Opus);
    }

    #[test]
    fn missing_key_reports_unavailable() {
        let exec = MockExecutor::new(Vec::new());
        let asr = OpenAiCompatAsr::new("openai", "https://api.openai.com/v1", "", "whisper-1", exec);
        assert!(!asr.is_available());
    }
}

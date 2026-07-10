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
//! network-free and fully unit-testable; the deacon supplies a reqwest-backed
//! executor. Request building and response parsing are pure functions (here);
//! the ASR/TTS provider impls live in `asr` / `tts`.

mod asr;
mod tts;
#[cfg(test)]
mod tests;

pub use asr::OpenAiCompatAsr;
pub use tts::OpenAiCompatTts;

use regent_kernel::TtsOptions;

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
/// Implemented by the deacon over reqwest; mocked in tests.
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

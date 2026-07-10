//! Request-building + provider round-trip tests over a mock executor.

use super::*;
use regent_kernel::{AsrOptions, AsrProvider, AudioBuffer, AudioFormat, TtsOptions, TtsProvider};
use std::sync::{Arc, Mutex};

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
    assert_eq!(
        req.url,
        "https://api.groq.com/openai/v1/audio/transcriptions"
    );
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
    let asr = OpenAiCompatAsr::new(
        "groq",
        "https://api.groq.com/openai/v1",
        "k",
        "whisper-1",
        Arc::clone(&exec),
    );
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
    assert_eq!(
        parse_transcription_response(b"  hello world \n"),
        "hello world"
    );
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

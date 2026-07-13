//! Unit tests for `speech` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn duration_ms_handles_mono_stereo_and_empty() {
    // 16 kHz mono, 16000 samples = 1000 ms.
    let mono = AudioBuffer::new(vec![0; 16_000], 16_000, 1);
    assert_eq!(mono.duration_ms(), 1000);
    // 16 kHz stereo, 32000 samples (16000 frames) = 1000 ms.
    let stereo = AudioBuffer::new(vec![0; 32_000], 16_000, 2);
    assert_eq!(stereo.duration_ms(), 1000);
    // Invalid sample_rate must not divide by zero.
    let bad = AudioBuffer::new(vec![0; 10], 0, 1);
    assert_eq!(bad.duration_ms(), 0);
}

#[test]
fn audio_format_serializes_lowercase_and_maps_extension() {
    assert_eq!(
        serde_json::to_string(&AudioFormat::Opus).unwrap(),
        "\"opus\""
    );
    assert_eq!(
        serde_json::from_str::<AudioFormat>("\"mp3\"").unwrap(),
        AudioFormat::Mp3
    );
    assert_eq!(AudioFormat::Ogg.ext(), "ogg");
    assert_eq!(AudioFormat::default(), AudioFormat::Mp3);
}

#[test]
fn provider_setup_round_trips_over_json() {
    let setup = ProviderSetup {
        display_name: "Groq".into(),
        badge: "free".into(),
        env_vars: vec![EnvVarPrompt {
            key: "GROQ_API_KEY".into(),
            prompt: "Groq API key".into(),
            url: Some("https://console.groq.com/keys".into()),
        }],
    };
    let json = serde_json::to_string(&setup).unwrap();
    assert_eq!(serde_json::from_str::<ProviderSetup>(&json).unwrap(), setup);
}

/// A minimal provider proves the traits are object-safe (`Box<dyn _>`) and
/// that the default methods (`is_available`, `list_*`, `setup_schema`,
/// `synthesize_streaming`) compile without being overridden.
struct EchoTts;
impl TtsProvider for EchoTts {
    fn name(&self) -> &str {
        "echo"
    }
    fn synthesize(&self, text: &str, opts: &TtsOptions) -> Result<SynthesizedAudio, RegentError> {
        Ok(SynthesizedAudio {
            bytes: text.as_bytes().to_vec(),
            format: opts.format,
        })
    }
}

#[test]
fn streaming_default_emits_full_audio_once() {
    let tts: Box<dyn TtsProvider> = Box::new(EchoTts);
    assert!(!tts.voice_compatible());
    assert!(tts.is_available());
    assert!(tts.list_voices().is_empty());

    let calls = AtomicUsize::new(0);
    let seen = std::sync::Mutex::new(Vec::new());
    let audio = tts
        .synthesize_streaming("hello", &TtsOptions::default(), &|chunk| {
            calls.fetch_add(1, Ordering::Relaxed);
            seen.lock().unwrap().extend_from_slice(chunk);
        })
        .unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(audio.bytes, b"hello");
    assert_eq!(&*seen.lock().unwrap(), b"hello");
}

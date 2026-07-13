//! Local ONNX engines via sherpa-onnx (`local-onnx` feature): whisper ASR +
//! Kokoro TTS. Models are file-system inputs — put the extracted sherpa-onnx
//! release folders under `REGENT_MODELS_DIR` (see [`super::engines::probe`]
//! for the expected layout); nothing downloads here.

use crate::domain::wav::parse_pcm16_mono;
use crate::infra::engines::{AsrEngine, ModelFiles, TtsEngine};
use regent_kernel::AudioBuffer;
use sherpa_rs::tts::{KokoroTts, KokoroTtsConfig};
use sherpa_rs::whisper::{WhisperConfig, WhisperRecognizer};
use std::sync::Mutex;

/// Whisper (sherpa offline recognizer). The language is fixed at load
/// (`REGENT_WHISPER_LANG`, empty = auto-detect on multilingual models) —
/// sherpa has no per-call hint, so the request's is ignored.
pub struct WhisperAsr {
    inner: Mutex<WhisperRecognizer>,
}

impl WhisperAsr {
    pub fn load(files: &ModelFiles) -> Result<Self, String> {
        let recognizer = WhisperRecognizer::new(WhisperConfig {
            encoder: files.encoder.clone(),
            decoder: files.decoder.clone(),
            tokens: files.tokens.clone(),
            language: std::env::var("REGENT_WHISPER_LANG").unwrap_or_default(),
            num_threads: Some(4),
            ..WhisperConfig::default()
        })
        .map_err(|e| e.to_string())?;
        Ok(Self {
            inner: Mutex::new(recognizer),
        })
    }
}

impl AsrEngine for WhisperAsr {
    fn transcribe(&self, audio: &[u8], _language: Option<&str>) -> Result<String, String> {
        let (rate, samples) = parse_pcm16_mono(audio)?;
        Ok(self.inner.lock().unwrap().transcribe(rate, &samples).text)
    }
}

/// Kokoro-82M (sherpa offline TTS). Voice via `REGENT_KOKORO_SPEAKER` (the
/// voices file's speaker index, default 0), re-read per synthesis so the
/// settings picker takes effect on the next reply.
pub struct KokoroEngine {
    inner: Mutex<KokoroTts>,
}

/// The settings pickers write `REGENT_KOKORO_SPEAKER`/`REGENT_KOKORO_SPEED`
/// to `$REGENT_HOME/.env` while this server is running — the file wins over
/// the (spawn-time) process env so a change speaks on the very next reply,
/// no restart. Reading a tiny file per spoken turn is free next to the TTS
/// inference itself.
fn live_env(key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    std::fs::read_to_string(super::spawn::regent_home().join(".env"))
        .ok()
        .and_then(|dotenv| {
            dotenv.lines().find_map(|line| {
                let rest = line.trim().strip_prefix(&prefix)?;
                Some(rest.trim().trim_matches('"').to_owned())
            })
        })
        .or_else(|| std::env::var(key).ok())
}

fn live_speaker() -> i32 {
    live_env("REGENT_KOKORO_SPEAKER")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// Speech rate for Kokoro's per-call `speed` arg (1.0 = normal). Clamped to
/// the same 0.5–2.0 range `voice.set` validates, so a hand-edited .env can't
/// ask sherpa for something absurd.
fn live_speed() -> f32 {
    live_env("REGENT_KOKORO_SPEED")
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|v| (0.5..=2.0).contains(v))
        .unwrap_or(1.0)
}

impl KokoroEngine {
    pub fn load(dir: &std::path::Path) -> Result<Self, String> {
        let need = |name: &str| {
            let p = dir.join(name);
            p.exists()
                .then(|| p.to_string_lossy().into_owned())
                .ok_or_else(|| format!("kokoro: missing {}", p.display()))
        };
        let optional = |name: &str| {
            let p = dir.join(name);
            if p.exists() {
                p.to_string_lossy().into_owned()
            } else {
                String::new()
            }
        };
        let tts = KokoroTts::new(KokoroTtsConfig {
            model: need("model.onnx")?,
            voices: need("voices.bin")?,
            tokens: need("tokens.txt")?,
            data_dir: need("espeak-ng-data")?,
            dict_dir: optional("dict"),
            lexicon: optional("lexicon-us-en.txt"),
            length_scale: 1.0,
            ..KokoroTtsConfig::default()
        });
        Ok(Self {
            inner: Mutex::new(tts),
        })
    }
}

impl TtsEngine for KokoroEngine {
    fn synthesize(&self, text: &str) -> Result<AudioBuffer, String> {
        let audio = self
            .inner
            .lock()
            .unwrap()
            .create(text, live_speaker(), live_speed())
            .map_err(|e| e.to_string())?;
        let samples = audio
            .samples
            .iter()
            .map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
            .collect();
        Ok(AudioBuffer::new(samples, audio.sample_rate, 1))
    }
}

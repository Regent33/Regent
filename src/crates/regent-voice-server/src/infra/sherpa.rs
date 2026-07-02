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
/// voices file's speaker index, default 0).
pub struct KokoroEngine {
    inner: Mutex<KokoroTts>,
    speaker: i32,
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
        let speaker = std::env::var("REGENT_KOKORO_SPEAKER")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        Ok(Self {
            inner: Mutex::new(tts),
            speaker,
        })
    }
}

impl TtsEngine for KokoroEngine {
    fn synthesize(&self, text: &str) -> Result<AudioBuffer, String> {
        let audio = self
            .inner
            .lock()
            .unwrap()
            .create(text, self.speaker, 1.0)
            .map_err(|e| e.to_string())?;
        let samples = audio
            .samples
            .iter()
            .map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
            .collect();
        Ok(AudioBuffer::new(samples, audio.sample_rate, 1))
    }
}

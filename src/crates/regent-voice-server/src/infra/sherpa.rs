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

/// Whisper (sherpa offline recognizer). `REGENT_WHISPER_LANG` (process env or
/// `$REGENT_HOME/.env`) wins; otherwise the caller's two-letter locale hint is
/// used. An explicit empty setting means auto-detect. Sherpa fixes language at
/// recognizer construction, so a live setting change rebuilds it once before
/// the next turn instead of silently requiring a server restart.
pub struct WhisperAsr {
    files: ModelFiles,
    inner: Mutex<WhisperState>,
}

struct WhisperState {
    recognizer: WhisperRecognizer,
    language: String,
}

impl WhisperAsr {
    pub fn load(files: &ModelFiles) -> Result<Self, String> {
        let language = whisper_language(None);
        let recognizer = make_whisper(files, &language)?;
        Ok(Self {
            files: files.clone(),
            inner: Mutex::new(WhisperState {
                recognizer,
                language,
            }),
        })
    }
}

fn make_whisper(files: &ModelFiles, language: &str) -> Result<WhisperRecognizer, String> {
    WhisperRecognizer::new(WhisperConfig {
        encoder: files.encoder.clone(),
        decoder: files.decoder.clone(),
        tokens: files.tokens.clone(),
        language: language.to_owned(),
        num_threads: Some(4),
        // sherpa-onnx's recommended Whisper default is -1 (automatic tail
        // padding). sherpa-rs otherwise substitutes 0, which gives the model no
        // tail frames and clips/garbles the final word at our VAD endpoint.
        tail_paddings: Some(-1),
        ..WhisperConfig::default()
    })
    .map_err(|e| e.to_string())
}

fn whisper_language(hint: Option<&str>) -> String {
    resolve_whisper_language(live_env("REGENT_WHISPER_LANG"), hint)
}

fn resolve_whisper_language(configured: Option<String>, hint: Option<&str>) -> String {
    configured.unwrap_or_else(|| {
        hint.filter(|value| {
            value.len() == 2 && value.bytes().all(|byte| byte.is_ascii_alphabetic())
        })
        .map(str::to_ascii_lowercase)
        .unwrap_or_default()
    })
}

impl AsrEngine for WhisperAsr {
    fn transcribe(&self, audio: &[u8], language: Option<&str>) -> Result<String, String> {
        let (rate, samples) = parse_pcm16_mono(audio)?;
        let desired = whisper_language(language);
        let mut state = self.inner.lock().unwrap();
        if state.language != desired {
            state.recognizer = make_whisper(&self.files, &desired)?;
            state.language = desired;
        }
        Ok(state.recognizer.transcribe(rate, &samples).text)
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

    fn cache_key(&self) -> String {
        format!("kokoro:{}:{:.3}", live_speaker(), live_speed())
    }
}

#[cfg(test)]
mod whisper_tests {
    use super::resolve_whisper_language;

    #[test]
    fn explicit_setting_wins_and_empty_setting_preserves_auto_detect() {
        assert_eq!(
            resolve_whisper_language(Some("ja".into()), Some("en")),
            "ja"
        );
        assert_eq!(
            resolve_whisper_language(Some(String::new()), Some("en")),
            ""
        );
    }

    #[test]
    fn safe_caller_hint_is_used_only_without_a_setting() {
        assert_eq!(resolve_whisper_language(None, Some("EN")), "en");
        assert_eq!(resolve_whisper_language(None, Some("en-US")), "");
    }
}

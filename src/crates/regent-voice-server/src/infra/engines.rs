//! ASR/TTS engine ports. The server logic depends only on these traits; the
//! local ONNX implementations (sherpa-onnx: whisper + Kokoro) live behind the
//! `local-onnx` cargo feature — a base build reports why speech is
//! unavailable instead of failing silently.

use regent_kernel::AudioBuffer;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Speech → text. Blocking (called via `spawn_blocking`).
pub trait AsrEngine: Send + Sync {
    /// Transcribe an audio container's bytes (WAV from the call UI).
    /// `language` is a hint, `None` = auto.
    fn transcribe(&self, audio: &[u8], language: Option<&str>) -> Result<String, String>;
}

/// Text → speech. Blocking (called via `spawn_blocking`).
pub trait TtsEngine: Send + Sync {
    fn synthesize(&self, text: &str) -> Result<AudioBuffer, String>;
}

/// The loaded engine pair — either may be absent, with the reason kept for
/// `/health` and the console (never a silent fallback).
#[derive(Clone, Default)]
pub struct Engines {
    pub asr: Option<Arc<dyn AsrEngine>>,
    pub tts: Option<Arc<dyn TtsEngine>>,
    /// Why an engine is missing (shown in /health and at startup).
    pub note: String,
}

impl Engines {
    #[must_use]
    pub fn unavailable(note: &str) -> Self {
        Self {
            asr: None,
            tts: None,
            note: note.to_owned(),
        }
    }

    #[must_use]
    pub fn ready(&self) -> bool {
        self.asr.is_some() && self.tts.is_some()
    }

    /// Load the local engines this build carries. Heavy (model load + a
    /// possible first-run download) — call off the request path. `progress`
    /// receives live status lines (surfaced via /health and the call UI).
    /// Without the `local-onnx` feature, explains how to get engines.
    #[must_use]
    pub fn from_env_with(progress: &dyn Fn(String)) -> Self {
        #[cfg(feature = "local-onnx")]
        {
            load_local(progress)
        }
        #[cfg(not(feature = "local-onnx"))]
        {
            let _ = progress;
            Self::unavailable(
                "this build has no local engines — rebuild with `--features local-onnx` \
                 (cargo build -p regent-voice-server --release --features local-onnx)",
            )
        }
    }

    #[must_use]
    pub fn from_env() -> Self {
        Self::from_env_with(&|_| {})
    }
}

/// The whisper model files sherpa needs, resolved from a release folder.
pub struct ModelFiles {
    pub encoder: String,
    pub decoder: String,
    pub tokens: String,
}

/// Where models live: `REGENT_MODELS_DIR` (default `tts-asr-local-models`).
#[must_use]
pub fn models_dir() -> PathBuf {
    PathBuf::from(
        std::env::var("REGENT_MODELS_DIR").unwrap_or_else(|_| "tts-asr-local-models".into()),
    )
}

/// Find whisper files in an extracted sherpa-onnx whisper release folder
/// (`*-encoder.int8.onnx` preferred over f32, `*-tokens.txt` or `tokens.txt`).
#[must_use]
pub fn probe_whisper(dir: &Path) -> Option<ModelFiles> {
    let names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    let pick = |suffix: &str| {
        names
            .iter()
            .find(|n| n.ends_with(&format!("{suffix}.int8.onnx")))
            .or_else(|| {
                names
                    .iter()
                    .find(|n| n.ends_with(&format!("{suffix}.onnx")))
            })
            .map(|n| dir.join(n).to_string_lossy().into_owned())
    };
    let tokens = names
        .iter()
        .find(|n| n.ends_with("tokens.txt"))
        .map(|n| dir.join(n).to_string_lossy().into_owned())?;
    Some(ModelFiles {
        encoder: pick("-encoder")?,
        decoder: pick("-decoder")?,
        tokens,
    })
}

#[cfg(feature = "local-onnx")]
fn load_local(progress: &dyn Fn(String)) -> Engines {
    use crate::infra::sherpa::{KokoroEngine, WhisperAsr};

    let base = models_dir();
    let size = std::env::var("REGENT_WHISPER_SIZE").unwrap_or_else(|_| "small".into());
    // First run: fetch the sherpa bundles (skips folders that exist).
    let mut notes: Vec<String> = crate::infra::download::ensure_models(&base, &size, progress);
    progress("loading local engines (whisper + kokoro)…".into());

    let whisper_dir = std::env::var("REGENT_WHISPER_DIR").map_or_else(
        |_| base.join(format!("sherpa-onnx-whisper-{size}")),
        PathBuf::from,
    );
    let asr: Option<Arc<dyn AsrEngine>> = match probe_whisper(&whisper_dir) {
        Some(files) => match WhisperAsr::load(&files) {
            Ok(engine) => Some(Arc::new(engine)),
            Err(e) => {
                notes.push(format!("whisper load failed: {e}"));
                None
            }
        },
        None => {
            notes.push(format!(
                "no whisper model in {} (put an extracted sherpa-onnx whisper release there, \
                 or set REGENT_WHISPER_DIR)",
                whisper_dir.display()
            ));
            None
        }
    };

    let kokoro_dir = std::env::var("REGENT_KOKORO_DIR")
        .map_or_else(|_| base.join("kokoro-en-v0_19"), PathBuf::from);
    let tts: Option<Arc<dyn TtsEngine>> = match KokoroEngine::load(&kokoro_dir) {
        Ok(engine) => Some(Arc::new(engine)),
        Err(e) => {
            notes.push(format!(
                "{e} (extract a sherpa-onnx kokoro release there, or set \
                                REGENT_KOKORO_DIR)"
            ));
            None
        }
    };

    Engines {
        asr,
        tts,
        note: if notes.is_empty() {
            "local engines ready".into()
        } else {
            notes.join(" · ")
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whisper_probe_prefers_int8_and_needs_all_three_files() {
        let dir = tempfile::tempdir().unwrap();
        let touch = |n: &str| std::fs::write(dir.path().join(n), b"x").unwrap();
        touch("tiny.en-encoder.onnx");
        touch("tiny.en-decoder.onnx");
        assert!(probe_whisper(dir.path()).is_none(), "tokens missing");
        touch("tiny.en-tokens.txt");
        touch("tiny.en-encoder.int8.onnx");
        let files = probe_whisper(dir.path()).unwrap();
        assert!(
            files.encoder.ends_with("tiny.en-encoder.int8.onnx"),
            "int8 preferred"
        );
        assert!(files.decoder.ends_with("tiny.en-decoder.onnx"));
    }
}

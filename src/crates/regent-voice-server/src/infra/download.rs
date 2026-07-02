//! First-run model download (`local-onnx` feature): fetch + extract the
//! sherpa-onnx release bundles into the models dir, exactly like the Python
//! server fetched Kokoro on first run. Skipped when the folder already
//! exists, or entirely with `REGENT_VOICE_AUTODOWNLOAD=0`. NOTE: the old
//! kokoro-v1.0/piper/Qwen3 files in the models dir are different formats —
//! the sherpa engines can't read them, so this download is genuinely needed.

use std::fs::File;
use std::io::Read;
use std::path::Path;

const ASR_BASE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models";
const TTS_BASE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models";

/// Ensure the whisper + kokoro bundles exist under `base`, reporting live
/// progress (shown in /health and the call UI while loading). Returns notes
/// for anything that failed.
pub fn ensure_models(base: &Path, whisper_size: &str, progress: &dyn Fn(String)) -> Vec<String> {
    let auto = std::env::var("REGENT_VOICE_AUTODOWNLOAD")
        .map(|v| !matches!(v.trim(), "0" | "false" | "no"))
        .unwrap_or(true);
    if !auto {
        return Vec::new();
    }
    let mut notes = Vec::new();
    let bundles = [
        (
            format!("sherpa-onnx-whisper-{whisper_size}"),
            format!("{ASR_BASE}/sherpa-onnx-whisper-{whisper_size}.tar.bz2"),
        ),
        (
            "kokoro-en-v0_19".to_owned(),
            format!("{TTS_BASE}/kokoro-en-v0_19.tar.bz2"),
        ),
    ];
    for (dir_name, url) in bundles {
        if base.join(&dir_name).exists() {
            continue;
        }
        progress(format!("downloading {dir_name} (one-time)…"));
        if let Err(e) = fetch_and_unpack(&url, base, &dir_name, progress) {
            notes.push(format!("{dir_name} download failed: {e}"));
        } else {
            progress(format!("{dir_name} ready"));
        }
    }
    notes
}

/// Counts bytes read through it and reports coarse progress.
struct Counting<'a, R> {
    inner: R,
    read: u64,
    last_report: u64,
    total_mb: Option<u64>,
    name: &'a str,
    progress: &'a dyn Fn(String),
}

impl<R: Read> Read for Counting<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read += n as u64;
        if self.read - self.last_report >= 25_000_000 {
            self.last_report = self.read;
            let got = self.read / 1_000_000;
            (self.progress)(match self.total_mb {
                Some(total) => format!("downloading {} — {got}/{total} MB", self.name),
                None => format!("downloading {} — {got} MB", self.name),
            });
        }
        Ok(n)
    }
}

/// Stream a .tar.bz2 to disk, then unpack into a temp dir and move the bundle
/// folder into place — a killed process never leaves a half-extracted folder
/// that the probe would mistake for an install.
fn fetch_and_unpack(
    url: &str,
    base: &Path,
    name: &str,
    progress: &dyn Fn(String),
) -> Result<(), String> {
    std::fs::create_dir_all(base).map_err(|e| e.to_string())?;
    let resp = reqwest::blocking::Client::builder()
        .timeout(None) // large files on slow links — the read itself streams
        .build()
        .map_err(|e| e.to_string())?
        .get(url)
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let total_mb = resp.content_length().map(|l| l / 1_000_000);
    let mut counted = Counting {
        inner: resp,
        read: 0,
        last_report: 0,
        total_mb,
        name,
        progress,
    };
    let tmp_file = base.join(format!(".download-{name}.tar.bz2"));
    let tmp_dir = base.join(format!(".extract-{name}"));
    let result = (|| {
        let mut file = File::create(&tmp_file).map_err(|e| e.to_string())?;
        std::io::copy(&mut counted, &mut file).map_err(|e| e.to_string())?;
        progress(format!("unpacking {name}…"));
        let _ = std::fs::remove_dir_all(&tmp_dir);
        let file = File::open(&tmp_file).map_err(|e| e.to_string())?;
        tar::Archive::new(bzip2::read::BzDecoder::new(file))
            .unpack(&tmp_dir)
            .map_err(|e| e.to_string())?;
        std::fs::rename(tmp_dir.join(name), base.join(name)).map_err(|e| e.to_string())
    })();
    let _ = std::fs::remove_file(&tmp_file);
    let _ = std::fs::remove_dir_all(&tmp_dir);
    result
}

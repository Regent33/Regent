//! First-run model download (`local-onnx` feature): fetch + extract the
//! sherpa-onnx release bundles into the models dir, exactly like the Python
//! server fetched Kokoro on first run. Skipped when the folder already
//! exists, or entirely with `REGENT_VOICE_AUTODOWNLOAD=0`.

use std::fs::File;
use std::path::Path;

const ASR_BASE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models";
const TTS_BASE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models";

/// Ensure the whisper + kokoro bundles exist under `base`. Returns notes for
/// anything that failed (missing models then surface via /health).
pub fn ensure_models(base: &Path, whisper_size: &str) -> Vec<String> {
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
        let dir = base.join(&dir_name);
        if dir.exists() {
            continue;
        }
        println!("  downloading {dir_name} (one-time)…");
        if let Err(e) = fetch_and_unpack(&url, base) {
            notes.push(format!("{dir_name} download failed: {e}"));
        } else {
            println!("  ✓ {dir_name} ready");
        }
    }
    notes
}

/// Stream a .tar.bz2 to disk, then unpack it into `base` (the archive's root
/// folder becomes `base/<name>/`). Temp file cleaned up either way.
fn fetch_and_unpack(url: &str, base: &Path) -> Result<(), String> {
    std::fs::create_dir_all(base).map_err(|e| e.to_string())?;
    let mut resp = reqwest::blocking::Client::builder()
        .timeout(None) // large files on slow links — the read itself streams
        .build()
        .map_err(|e| e.to_string())?
        .get(url)
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    if let Some(len) = resp.content_length() {
        println!("    {} MB…", len / 1_000_000);
    }
    let tmp = base.join(".download.tar.bz2");
    let result = (|| {
        let mut file = File::create(&tmp).map_err(|e| e.to_string())?;
        std::io::copy(&mut resp, &mut file).map_err(|e| e.to_string())?;
        let file = File::open(&tmp).map_err(|e| e.to_string())?;
        tar::Archive::new(bzip2::read::BzDecoder::new(file))
            .unpack(base)
            .map_err(|e| e.to_string())
    })();
    let _ = std::fs::remove_file(&tmp);
    result
}

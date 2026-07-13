//! OCR rung of the document ladder: when a PDF/deck yields near-empty text
//! (scanned pages, photo slides), PP-OCR reads the embedded images locally —
//! no provider, no key, no upload. Rides the same `ort` runtime the embedder
//! (fastembed) already ships; the three ONNX models (det + cls + rec, ~16 MB)
//! download once from Hugging Face into `$REGENT_HOME/models/ocr`.
// ponytail: ch_PP-OCRv4 models (they cover Latin + CJK and embed their own
// charset in the ONNX metadata); language-specific rec models if accuracy on
// some script ever disappoints.

use paddle_ocr_rs::ocr_lite::OcrLite;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Pinned model set — RapidOCR's ONNX exports of the official PaddleOCR
/// models (charset embedded in metadata, which `paddle-ocr-rs` requires).
const MODELS: [(&str, &str); 3] = [
    (
        "ch_PP-OCRv4_det_infer.onnx",
        "https://huggingface.co/SWHL/RapidOCR/resolve/main/PP-OCRv4/ch_PP-OCRv4_det_infer.onnx",
    ),
    (
        "ch_ppocr_mobile_v2.0_cls_infer.onnx",
        "https://huggingface.co/SWHL/RapidOCR/resolve/main/PP-OCRv1/ch_ppocr_mobile_v2.0_cls_infer.onnx",
    ),
    (
        "ch_PP-OCRv4_rec_infer.onnx",
        "https://huggingface.co/SWHL/RapidOCR/resolve/main/PP-OCRv4/ch_PP-OCRv4_rec_infer.onnx",
    ),
];

/// A download below this is an error page, not a model.
const MIN_MODEL_BYTES: u64 = 100_000;
const DOWNLOAD_TIMEOUT_SECS: u64 = 300;

/// Text shorter than this (after trim) marks an extraction as "near-empty" —
/// the signal that the document's content lives in images.
pub(super) const OCR_TEXT_FLOOR: usize = 200;

/// True when `text` is too thin to be the document's real content.
pub(super) fn needs_ocr(text: &str) -> bool {
    text.trim().chars().count() < OCR_TEXT_FLOOR
}

fn models_dir() -> PathBuf {
    let home = std::env::var("REGENT_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let user = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_default();
            PathBuf::from(user).join(".regent")
        });
    home.join("models").join("ocr")
}

/// Downloads any missing model into the cache dir; returns the three paths
/// (det, cls, rec). Every failure is a plain-language reason.
pub(super) async fn ensure_models() -> Result<[PathBuf; 3], String> {
    let dir = models_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("cannot create model cache {}: {e}", dir.display()))?;
    let mut paths: Vec<PathBuf> = Vec::with_capacity(3);
    for (name, url) in MODELS {
        let target = dir.join(name);
        let have = std::fs::metadata(&target)
            .map(|m| m.len() >= MIN_MODEL_BYTES)
            .unwrap_or(false);
        if !have {
            tracing::info!(model = name, "downloading OCR model");
            download(url, &target)
                .await
                .map_err(|reason| format!("OCR model {name} download failed: {reason}"))?;
        }
        paths.push(target);
    }
    Ok([paths[0].clone(), paths[1].clone(), paths[2].clone()])
}

async fn download(url: &str, target: &Path) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} from {url}", resp.status().as_u16()));
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    if (bytes.len() as u64) < MIN_MODEL_BYTES {
        return Err(format!(
            "{url} returned {} bytes — not a model",
            bytes.len()
        ));
    }
    // Temp-then-rename so a torn download never poses as a cached model.
    let tmp = target.with_extension("part");
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, target).map_err(|e| e.to_string())?;
    Ok(())
}

/// The engine loads once per process (~100 ms) and is reused; `detect` needs
/// `&mut`, so calls serialize behind the mutex — OCR is CPU-bound anyway.
static ENGINE: OnceLock<Mutex<OcrLite>> = OnceLock::new();

fn engine(models: &[PathBuf; 3]) -> Result<&'static Mutex<OcrLite>, String> {
    if let Some(existing) = ENGINE.get() {
        return Ok(existing);
    }
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get().min(4));
    let mut ocr = OcrLite::new();
    ocr.init_models(
        &models[0].to_string_lossy(),
        &models[1].to_string_lossy(),
        &models[2].to_string_lossy(),
        threads,
    )
    .map_err(|e| format!("OCR model init failed: {e}"))?;
    // A concurrent init losing this race is fine — one engine wins, both work.
    let _ = ENGINE.set(Mutex::new(ocr));
    Ok(ENGINE.get().expect("engine just set"))
}

/// OCRs each image file and returns the combined text (per-file headers when
/// there are several). BLOCKING — call from `spawn_blocking`; a model that
/// panics inside `paddle-ocr-rs` surfaces as the task's join error there.
pub(super) fn ocr_files(models: &[PathBuf; 3], files: &[PathBuf]) -> Result<String, String> {
    let engine = engine(models)?;
    let mut ocr = engine
        .lock()
        .map_err(|_| "OCR engine poisoned".to_owned())?;
    let mut out = String::new();
    let mut unreadable = 0usize;
    for file in files {
        let Ok(img) = image::open(file) else {
            unreadable += 1;
            continue;
        };
        // README-recommended thresholds; angle detection on — scans rotate.
        let result = ocr
            .detect(&img.to_rgb8(), 50, 1024, 0.5, 0.3, 1.6, true, false)
            .map_err(|e| format!("OCR failed on {}: {e}", file.display()))?;
        let text: String = result
            .text_blocks
            .iter()
            .map(|b| b.text.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if text.is_empty() {
            continue;
        }
        if files.len() > 1 {
            let name = file
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default();
            out.push_str(&format!("--- {name} ---\n"));
        }
        out.push_str(&text);
        out.push('\n');
    }
    if out.is_empty() {
        return Err(format!(
            "no text recognized across {} image(s) ({unreadable} unreadable)",
            files.len()
        ));
    }
    Ok(out.trim().to_owned())
}

#[cfg(test)]
#[path = "ocr_tests.rs"]
mod tests;

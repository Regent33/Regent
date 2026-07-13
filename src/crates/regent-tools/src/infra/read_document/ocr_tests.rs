//! Unit tests for `ocr` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

#[test]
fn the_ocr_gate_fires_only_on_near_empty_text() {
    assert!(needs_ocr(""));
    assert!(needs_ocr("   \n page 3 \n  "));
    assert!(!needs_ocr(&"real content ".repeat(30)));
}

/// End-to-end OCR against the real models — downloads ~16 MB on first
/// run, so opt-in: `cargo test -p regent-tools ocr_reads -- --ignored`.
/// Set `REGENT_OCR_TEST_IMG=<png with text>` to also assert recognition.
#[tokio::test]
#[ignore = "downloads models; run manually"]
async fn ocr_reads_rendered_text() {
    let models = ensure_models().await.expect("models download");
    // A blank page proves the engine loads and inference runs panic-free
    // (a white canvas legitimately recognizes nothing).
    let dir = tempfile::tempdir().expect("tempdir");
    let blank = dir.path().join("blank.png");
    image::RgbImage::from_pixel(320, 64, image::Rgb([255, 255, 255]))
        .save(&blank)
        .expect("save blank");
    let m = models.clone();
    let result = tokio::task::spawn_blocking(move || ocr_files(&m, &[blank]))
        .await
        .expect("join");
    assert!(result.is_err(), "blank page should recognize nothing");
    assert!(result.unwrap_err().contains("no text recognized"));

    // Real recognition, when the runner supplies a text image.
    if let Ok(sample) = std::env::var("REGENT_OCR_TEST_IMG") {
        let file = PathBuf::from(sample);
        let text = tokio::task::spawn_blocking(move || ocr_files(&models, &[file]))
            .await
            .expect("join")
            .expect("sample image should yield text");
        assert!(!text.trim().is_empty());
        println!("OCR read: {text}");
    }
}

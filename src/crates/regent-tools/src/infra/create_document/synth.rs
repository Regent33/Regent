//! Byte synthesis for `create_document`: PDF/PPTX prefer the themed renderer
//! sidecar (HTML→Chromium, PptxGenJS) and fall back to the native Rust writers
//! when it is absent; DOCX/XLSX are always native. Synthesis is either a
//! subprocess (async) or CPU-bound work offloaded to `spawn_blocking` — never on
//! the async runtime directly.

use super::model::{DocFormat, DocumentSpec};
use super::theme::{self, Theme};
use super::{deck, docx, html, pdf, pptx, renderer, xlsx};
use serde_json::json;

/// Produce the document bytes for `spec`.
pub async fn synthesize(spec: DocumentSpec) -> Result<Vec<u8>, String> {
    let theme = theme::resolve(spec.theme.as_ref(), theme_seed(&spec));
    match spec.format {
        DocFormat::Pdf => build_pdf(&spec, &theme).await,
        DocFormat::Pptx => build_pptx(&spec, &theme).await,
        DocFormat::Docx => run_native(spec, docx::build).await,
        DocFormat::Xlsx => run_native(spec, xlsx::build).await,
    }
}

async fn build_pdf(spec: &DocumentSpec, theme: &Theme) -> Result<Vec<u8>, String> {
    if renderer::find_renderer().is_some() {
        let html = html::report(spec, theme)?;
        renderer::render(&json!({ "kind": "pdf", "html": html })).await
    } else {
        run_native(spec.clone(), pdf::build).await
    }
}

async fn build_pptx(spec: &DocumentSpec, theme: &Theme) -> Result<Vec<u8>, String> {
    if renderer::find_renderer().is_some() {
        let deck = deck::build_spec(spec, theme);
        renderer::render(&json!({ "kind": "pptx", "deck": deck })).await
    } else {
        run_native(spec.clone(), pptx::build).await
    }
}

/// Offload a native (CPU-bound) writer to a blocking thread.
async fn run_native<F>(spec: DocumentSpec, build: F) -> Result<Vec<u8>, String>
where
    F: FnOnce(&DocumentSpec) -> Result<Vec<u8>, String> + Send + 'static,
{
    tokio::task::spawn_blocking(move || build(&spec))
        .await
        .map_err(|join| format!("generation task failed: {join}"))?
}

/// The string that seeds the default theme when the model names none — the most
/// content-identifying text available, so different documents diverge.
pub fn theme_seed(spec: &DocumentSpec) -> &str {
    spec.title
        .as_deref()
        .or_else(|| spec.sections.first().and_then(|s| s.heading.as_deref()))
        .or_else(|| spec.slides.first().map(|s| s.title.as_str()))
        .or_else(|| spec.sheets.first().map(|s| s.name.as_str()))
        .unwrap_or("regent")
}

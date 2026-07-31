//! Byte synthesis for `create_document`: PDF/PPTX prefer the themed renderer
//! sidecar (HTML→Chromium, PptxGenJS) and fall back to the native Rust writers
//! when it is absent; DOCX/XLSX are always native. Synthesis is either a
//! subprocess (async) or CPU-bound work offloaded to `spawn_blocking` — never on
//! the async runtime directly.
//!
//! All four formats now receive the same resolved `Theme`. DOCX and XLSX used to
//! be handed the spec alone, which is why every Word and Excel file Regent made
//! looked identical regardless of subject.

use super::model::{DocFormat, DocumentSpec};
use super::theme::{self, Theme};
use super::{deck, docx, html, pdf, pptx, renderer, xlsx};
use serde_json::json;

/// What the caller must tell the model when the designed renderer was missing.
/// A fallback document is a visibly poorer thing — one fixed layout, no theme,
/// no model-placed `elements` — and saying nothing let a plain deck read as
/// "this is the best Regent can do" for as long as the sidecar stayed unfound.
pub const FALLBACK_NOTE: &str = "the designed renderer was not found, so this file used the \
     plain built-in writer: no theme, no per-slide layouts, and any `elements` were dropped. \
     Build it with `bun run compile` in src/regent-cli, or set REGENT_CLI_PATH.";

/// The document bytes for `spec`, plus a note when output was degraded.
pub async fn synthesize(spec: DocumentSpec) -> Result<(Vec<u8>, Option<String>), String> {
    let theme = theme::resolve(spec.theme.as_ref(), theme_seed(&spec));
    let designed = renderer::find_renderer().is_some();
    let bytes = match spec.format {
        DocFormat::Pdf => build_pdf(&spec, &theme, designed).await,
        DocFormat::Pptx => build_pptx(&spec, &theme, designed).await,
        // Native by design, not by fallback — these two carry no renderer path.
        DocFormat::Docx => return Ok((run_native(spec, theme, docx::build).await?, None)),
        DocFormat::Xlsx => return Ok((run_native(spec, theme, xlsx::build).await?, None)),
    }?;
    if !designed {
        tracing::warn!(format = spec.format.as_str(), "{FALLBACK_NOTE}");
    }
    Ok((bytes, (!designed).then(|| FALLBACK_NOTE.to_owned())))
}

async fn build_pdf(spec: &DocumentSpec, theme: &Theme, designed: bool) -> Result<Vec<u8>, String> {
    if designed {
        // Model-authored markup wins over the built-in template: the whole
        // point of the escape hatch is that the model owns the layout.
        let html = match spec.html.as_deref() {
            Some(raw) => super::authored::as_document(super::authored::usable_html(raw)?),
            None => html::report(spec, theme)?,
        };
        renderer::render(&json!({ "kind": "pdf", "html": html })).await
    } else {
        run_native(spec.clone(), theme.clone(), |spec, _| pdf::build(spec)).await
    }
}

async fn build_pptx(spec: &DocumentSpec, theme: &Theme, designed: bool) -> Result<Vec<u8>, String> {
    if designed {
        let deck = deck::build_spec(spec, theme);
        renderer::render(&json!({ "kind": "pptx", "deck": deck })).await
    } else {
        run_native(spec.clone(), theme.clone(), |spec, _| pptx::build(spec)).await
    }
}

/// Offload a native (CPU-bound) writer to a blocking thread. Every writer takes
/// the resolved theme; the two fallback writers (native PDF, native PPTX) carry
/// their own fixed visual system and ignore it.
async fn run_native<F>(spec: DocumentSpec, theme: Theme, build: F) -> Result<Vec<u8>, String>
where
    F: FnOnce(&DocumentSpec, &Theme) -> Result<Vec<u8>, String> + Send + 'static,
{
    tokio::task::spawn_blocking(move || build(&spec, &theme))
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

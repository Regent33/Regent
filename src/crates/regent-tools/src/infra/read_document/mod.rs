//! `read_document` — text, hyperlink, and embedded-image extraction for
//! PDF / Word / PowerPoint / Excel, natively in-process. Born from a real
//! failure: with no document tool the model fell back to `python3 -c`
//! one-liners, and on Windows `python3` is often a store shim that hangs and
//! returns nothing (sess_7d79…, 2026-07-13). Embedded images are written to
//! the session scratch area so the model can `vision_analyze` them (that is
//! the path that ships them to the vision provider).
// ponytail: PDF gives text only — its links/images need a rasterizer/annot
// walk; add when a real task needs them. Office media + rels cover the
// common "the deck is mostly pictures" case.

mod direct;
mod extractors;
mod media;
mod ocr;
mod pdf_images;

use crate::ToolCatalog;
use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "read_document".into(),
        // Kept to the 60-char defer-hook window: this whole line IS the
        // model's index entry while the tool is deferred.
        description: "Extract text, links, images from PDF/Word/PowerPoint/Excel.".into(),
        parameters: json!({
            "type": "object",
            "properties": {"path": {"type": "string"}},
            "required": ["path"]
        }),
        toolset: "file".into(),
    }
}

struct ReadDocumentTool;

#[async_trait]
impl ToolExecutor for ReadDocumentTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(path) = args.get("path").and_then(Value::as_str) else {
            return Ok(tool_error_json("missing required parameter: path"));
        };
        let resolved = match ctx.resolve(path) {
            Ok(resolved) => resolved,
            Err(error) => return Ok(tool_error_json(error.to_string())),
        };
        let ext = resolved
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        // Ladder rung 1: the model reads the PDF ITSELF (pictures, layout,
        // links included). Rungs 2–3 below: local extraction + embedded
        // images for vision_analyze, degrading to text-only. A provider that
        // can't take file inputs fails with a NAMED reason that rides along
        // in the fallback result — never a silent downgrade.
        let mut direct_skipped: Option<String> = None;
        if ext == "pdf" {
            match direct::model_reads_pdf(&resolved).await {
                Ok(text) => {
                    return Ok(
                        json!({"format": "pdf", "source": "model-direct", "text": text})
                            .to_string(),
                    );
                }
                Err(reason) => {
                    tracing::info!(%reason, "direct model read unavailable — local extraction");
                    direct_skipped = Some(reason);
                }
            }
        }
        let media_dir = ctx.scratch_dir.clone();
        // Extraction is CPU-bound file work — off the async runtime.
        let (blocking_path, blocking_ext) = (resolved.clone(), ext.clone());
        let outcome = tokio::task::spawn_blocking(move || {
            extractors::extract(&blocking_path, &blocking_ext, media_dir.as_deref())
        })
        .await
        .map_err(|join| RegentError::Tool {
            tool: "read_document".to_owned(),
            message: format!("extraction task failed: {join}"),
        })?;
        Ok(match outcome {
            Ok(mut value) => {
                if let Some(reason) = direct_skipped {
                    value["model_direct"] = json!(format!("skipped: {reason}"));
                }
                // Ladder's last rung: near-empty text means the content lives
                // in images (scanned PDF, photo deck) — local OCR reads them.
                maybe_ocr(&mut value, &resolved, &ext, ctx.scratch_dir.as_deref()).await;
                value.to_string()
            }
            // Both rungs failed — the error names both reasons, so a scanned
            // PDF on a text-only provider is diagnosable from the result alone.
            Err(message) => match direct_skipped {
                Some(reason) => tool_error_json(format!(
                    "{message} (model-direct read also failed: {reason})"
                )),
                None => tool_error_json(message),
            },
        })
    }
}

/// OCR rung: fires only when extracted text is near-empty. Reads the OOXML
/// media already extracted, plus a PDF's embedded page images. Every failure
/// degrades to an `ocr: skipped …` field — the read itself never fails here.
async fn maybe_ocr(value: &mut Value, path: &Path, ext: &str, scratch: Option<&Path>) {
    let thin = value["text"].as_str().is_none_or(ocr::needs_ocr);
    if !thin {
        return;
    }
    let mut files: Vec<std::path::PathBuf> = value["images"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|v| v.as_str().map(std::path::PathBuf::from))
                .collect()
        })
        .unwrap_or_default();
    if ext == "pdf" {
        let Some(dir) = scratch else {
            value["ocr"] = json!("skipped: no scratch area to extract page images into");
            return;
        };
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("document");
        let out_dir = dir.join(format!("{stem}-media"));
        let pdf = path.to_owned();
        match tokio::task::spawn_blocking(move || pdf_images::extract_pdf_images(&pdf, &out_dir))
            .await
        {
            Ok(Ok(images)) => files.extend(images),
            Ok(Err(reason)) => {
                value["ocr"] = json!(format!("skipped: {reason}"));
                return;
            }
            Err(join) => {
                value["ocr"] = json!(format!("skipped: image extraction crashed: {join}"));
                return;
            }
        }
    }
    if files.is_empty() {
        value["ocr"] = json!("skipped: no embedded images found to OCR");
        return;
    }
    let models = match ocr::ensure_models().await {
        Ok(models) => models,
        Err(reason) => {
            value["ocr"] = json!(format!("skipped: {reason}"));
            return;
        }
    };
    // spawn_blocking doubles as panic isolation — paddle-ocr-rs panics on a
    // model without embedded charset metadata; that lands here as a JoinError.
    match tokio::task::spawn_blocking(move || ocr::ocr_files(&models, &files)).await {
        Ok(Ok(text)) => {
            value["text"] = json!(text);
            value["source"] = json!("local-ocr");
            value["note"] = json!(
                "text recovered by local OCR (PP-OCRv4) from the document's images — \
                 reading order is approximate"
            );
        }
        Ok(Err(reason)) => value["ocr"] = json!(format!("ran, {reason}")),
        Err(join) => value["ocr"] = json!(format!("skipped: OCR crashed: {join}")),
    }
}

/// Registers `read_document` on the catalog.
pub fn register_read_document_tool(catalog: &mut ToolCatalog) -> Result<(), RegentError> {
    catalog.register(definition(), Arc::new(ReadDocumentTool))
}

#[cfg(test)]
mod tests;

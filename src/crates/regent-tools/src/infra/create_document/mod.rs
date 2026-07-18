//! `create_document` — generate PDF / Word / PowerPoint / Excel files natively
//! in-process, the write-side twin of `read_document`. Same motivation: with no
//! document tool the model fell back to `python3 -c` one-liners that hang on
//! Windows. One flat content spec (`model::DocumentSpec`) drives all four
//! writers; `format` picks which of `sections` / `slides` / `sheets` is
//! authoritative. Every writer builds bytes in memory (off the async runtime,
//! under `spawn_blocking`) and this module writes them once, jailed through the
//! same `ToolContext::resolve` every file tool uses.

mod docx;
mod model;
mod pdf;
mod pptx;
mod pptx_scaffold;
mod pptx_xml;
mod xlsx;

use crate::ToolCatalog;
use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use model::{DocFormat, DocumentSpec, EmbeddedSlideImage};
use regent_kernel::{RegentError, ToolDefinition, tool_error_json, tool_result_json};
use serde_json::{Value, json};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "create_document".into(),
        description: "Create a PDF/Word/designed PowerPoint/Excel file from structured content. \
                      PowerPoint slides support subtitles, speaker notes, and optional local PNG/JPEG \
                      images. A relative path saves under the artifacts directory; a bare PPTX filename \
                      automatically gets its own presentation folder. Pass an absolute path only when \
                      the user names a specific location."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "format": {"type": "string", "enum": ["pdf", "docx", "pptx", "xlsx"]},
                "path": {"type": "string", "description": "Output file path."},
                "title": {"type": "string"},
                "sections": {
                    "type": "array",
                    "description": "Prose blocks — drive pdf/docx.",
                    "items": {"type": "object", "properties": {
                        "heading": {"type": "string"},
                        "paragraphs": {"type": "array", "items": {"type": "string"}},
                        "bullets": {"type": "array", "items": {"type": "string"}}
                    }}
                },
                "slides": {
                    "type": "array",
                    "description": "Slides — drive pptx.",
                    "items": {"type": "object", "properties": {
                        "title": {"type": "string"},
                        "subtitle": {"type": "string"},
                        "bullets": {"type": "array", "items": {"type": "string"}},
                        "notes": {"type": "string"},
                        "image": {
                            "type": "object",
                            "description": "Optional local PNG/JPEG visual generated or extracted for this slide.",
                            "properties": {
                                "path": {"type": "string"},
                                "alt_text": {"type": "string"}
                            },
                            "required": ["path"]
                        }
                    }, "required": ["title"]}
                },
                "sheets": {
                    "type": "array",
                    "description": "Worksheets — drive xlsx.",
                    "items": {"type": "object", "properties": {
                        "name": {"type": "string"},
                        "rows": {"type": "array", "items": {"type": "array"}},
                        "header": {"type": "boolean"}
                    }, "required": ["name", "rows"]}
                }
            },
            "required": ["format", "path"]
        }),
        toolset: "documents".into(),
    }
}

struct CreateDocumentTool;

#[async_trait]
impl ToolExecutor for CreateDocumentTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let mut spec: DocumentSpec = match serde_json::from_value(args) {
            Ok(spec) => spec,
            Err(error) => {
                return Ok(tool_error_json(format!(
                    "invalid create_document spec: {error}"
                )));
            }
        };
        if let Err(message) = spec.validate() {
            return Ok(tool_error_json(message));
        }
        if let Err(message) = hydrate_slide_images(&mut spec, ctx).await {
            return Ok(tool_error_json(message));
        }
        // Bug #10: a relative path lands in the ARTIFACTS area, not the
        // deacon's launch cwd — the prompt already points the model there,
        // but steering is not enforcement. Absolute paths are honored as-is
        // (still jail-checked); contexts without an artifacts dir (CLI/repl,
        // tests) keep the old cwd-relative behavior. `has_root` matters on
        // Windows: `\x` is not "absolute" (no drive) but Path::join would let
        // it REPLACE the artifacts base — rooted paths never join.
        let p = Path::new(&spec.path);
        let target = match (&ctx.artifacts_dir, p.is_relative() && !p.has_root()) {
            (Some(artifacts), true) => artifacts
                .join(artifact_relative_path(&spec))
                .display()
                .to_string(),
            _ => spec.path.clone(),
        };
        let resolved = match ctx.resolve(&target) {
            Ok(resolved) => resolved,
            Err(error) => return Ok(tool_error_json(error.to_string())),
        };
        if let Some(parent) = resolved.parent()
            && let Err(error) = tokio::fs::create_dir_all(parent).await
        {
            return Ok(tool_error_json(format!(
                "cannot create parent directory {}: {error}",
                parent.display()
            )));
        }

        // Document synthesis is CPU-bound (zip deflate, PDF encode) — off the
        // async runtime, mirroring read_document's extraction.
        let format = spec.format;
        let bytes = tokio::task::spawn_blocking(move || build(&spec))
            .await
            .map_err(|join| RegentError::Tool {
                tool: "create_document".to_owned(),
                message: format!("generation task failed: {join}"),
            })?;
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(message) => return Ok(tool_error_json(message)),
        };

        match tokio::fs::write(&resolved, &bytes).await {
            Ok(()) => Ok(tool_result_json(json!({
                "created": resolved.display().to_string(),
                "folder": resolved.parent().map(|path| path.display().to_string()),
                "format": format.as_str(),
                "bytes": bytes.len(),
            }))),
            Err(error) => Ok(tool_error_json(format!(
                "cannot write {}: {error}",
                resolved.display()
            ))),
        }
    }
}

/// A bare deck filename is ambiguous in a shared artifact root. Put it in a
/// deterministic, human-readable folder while preserving explicitly supplied
/// subfolders for callers that already organize their output.
fn artifact_relative_path(spec: &DocumentSpec) -> PathBuf {
    let path = Path::new(&spec.path);
    if spec.format != DocFormat::Pptx || path.components().count() != 1 {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("presentation");
    PathBuf::from(slug(stem)).join(path)
}

fn slug(value: &str) -> String {
    let mut out = String::new();
    let mut separator = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            if separator && !out.is_empty() {
                out.push('-');
            }
            out.push(ch.to_ascii_lowercase());
            separator = false;
        } else {
            separator = true;
        }
    }
    if out.is_empty() {
        "presentation".to_owned()
    } else {
        out
    }
}

async fn hydrate_slide_images(spec: &mut DocumentSpec, ctx: &ToolContext) -> Result<(), String> {
    if spec.format != DocFormat::Pptx {
        return Ok(());
    }
    for (index, slide) in spec.slides.iter_mut().enumerate() {
        let Some(image_spec) = &slide.image else {
            continue;
        };
        let resolved = ctx
            .resolve(&image_spec.path)
            .map_err(|error| error.to_string())?;
        let bytes = tokio::fs::read(&resolved).await.map_err(|error| {
            format!(
                "cannot read slide {} image {}: {error}",
                index + 1,
                resolved.display()
            )
        })?;
        let format = image::guess_format(&bytes).map_err(|error| {
            format!(
                "slide {} image is not a supported raster image: {error}",
                index + 1
            )
        })?;
        let decoded = image::load_from_memory_with_format(&bytes, format).map_err(|error| {
            format!(
                "cannot decode slide {} image {}: {error}",
                index + 1,
                resolved.display()
            )
        })?;
        let (bytes, extension, content_type) = match format {
            image::ImageFormat::Png => (bytes, "png", "image/png"),
            image::ImageFormat::Jpeg => (bytes, "jpg", "image/jpeg"),
            _ => {
                let mut encoded = Cursor::new(Vec::new());
                decoded
                    .write_to(&mut encoded, image::ImageFormat::Png)
                    .map_err(|error| {
                        format!("cannot convert slide {} image to PNG: {error}", index + 1)
                    })?;
                (encoded.into_inner(), "png", "image/png")
            }
        };
        slide.embedded_image = Some(EmbeddedSlideImage {
            bytes,
            extension,
            content_type,
            width: decoded.width(),
            height: decoded.height(),
            alt_text: image_spec
                .alt_text
                .clone()
                .unwrap_or_else(|| slide.title.clone()),
        });
    }
    Ok(())
}

/// Dispatches to the per-format byte builder.
fn build(spec: &DocumentSpec) -> Result<Vec<u8>, String> {
    match spec.format {
        DocFormat::Pdf => pdf::build(spec),
        DocFormat::Docx => docx::build(spec),
        DocFormat::Pptx => pptx::build(spec),
        DocFormat::Xlsx => xlsx::build(spec),
    }
}

/// Registers `create_document` on the catalog.
pub fn register_create_document_tool(catalog: &mut ToolCatalog) -> Result<(), RegentError> {
    catalog.register(definition(), Arc::new(CreateDocumentTool))
}

#[cfg(test)]
#[path = "create_document_tests.rs"]
mod tests;

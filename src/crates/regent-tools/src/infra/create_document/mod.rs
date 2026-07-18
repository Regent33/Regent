//! `create_document` — generate PDF / Word / PowerPoint / Excel files natively
//! in-process, the write-side twin of `read_document`. Same motivation: with no
//! document tool the model fell back to `python3 -c` one-liners that hang on
//! Windows. One flat content spec (`model::DocumentSpec`) drives all four
//! writers; `format` picks which of `sections` / `slides` / `sheets` is
//! authoritative. Every writer builds bytes in memory (off the async runtime,
//! under `spawn_blocking`) and this module writes them once, jailed through the
//! same `ToolContext::resolve` every file tool uses.

mod deck;
mod docx;
mod edit;
mod html;
mod images;
mod model;
mod pdf;
mod pptx;
mod pptx_scaffold;
mod pptx_xml;
mod preview;
mod renderer;
mod theme;
mod xlsx;

use crate::ToolCatalog;
use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use model::{DocFormat, DocumentSpec};
use regent_kernel::{RegentError, ToolDefinition, tool_error_json, tool_result_json};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use theme::Theme;

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "create_document".into(),
        description: "Create or edit a PDF/Word/designed PowerPoint/Excel file from structured content. \
                      PowerPoint slides support subtitles, speaker notes, and optional local PNG/JPEG \
                      images. A relative path saves under the artifacts directory; a bare PPTX filename \
                      automatically gets its own presentation folder. Pass an absolute path only when \
                      the user names a specific location. To revise a file this tool created earlier, \
                      pass `operation: \"edit\"` with the same `path` and a `patch` of just the fields \
                      to change — the rest is reloaded from the document's saved spec."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "format": {"type": "string", "enum": ["pdf", "docx", "pptx", "xlsx"]},
                "path": {"type": "string", "description": "Output file path. For an edit, the path returned when the file was created."},
                "operation": {
                    "type": "string",
                    "enum": ["create", "edit"],
                    "description": "Defaults to 'create'. 'edit' reloads the saved spec of a file this tool made and applies `patch`."
                },
                "patch": {
                    "type": "object",
                    "description": "Edit only: a JSON merge-patch over the saved spec. Set a field to change it, or to null to remove it; arrays replace wholesale (resupply the full slides/sections/sheets array to change one)."
                },
                "preview": {
                    "type": "boolean",
                    "description": "When true, also render a background <file>.preview.png (headless — no window) so you can `vision_analyze` it and fix layout/contrast/overflow, then edit. PDF screenshots the report; PPTX needs LibreOffice installed (else a note is returned)."
                },
                "title": {"type": "string"},
                "theme": {
                    "description": "Optional look, applied to pdf + pptx. Either a preset name \
                                    (midnight | warm-editorial | mono | forest | royal) or a custom \
                                    palette object with any of: background, text, accent, muted, \
                                    coverBackground, coverText, titleFont, bodyFont (6-hex colors, \
                                    no '#'), plus optional `base` preset to inherit from. Omitted → \
                                    a theme is derived from the content so documents differ."
                },
                "sections": {
                    "type": "array",
                    "description": "Prose blocks — drive pdf/docx.",
                    "items": {"type": "object", "properties": {
                        "heading": {"type": "string"},
                        "paragraphs": {"type": "array", "items": {"type": "string"}},
                        "bullets": {"type": "array", "items": {"type": "string"}},
                        "image": {
                            "type": "object",
                            "description": "Optional figure for this section (PDF only), sourced by ONE of \
                                            `query` (keyless photo lookup), `url`, or `path` — same as slide \
                                            images. Add `alt_text`. Unresolvable sources land in `image_notes`.",
                            "properties": {
                                "query": {"type": "string"},
                                "url": {"type": "string"},
                                "path": {"type": "string"},
                                "alt_text": {"type": "string"}
                            }
                        }
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
                        "layout": {
                            "type": "string",
                            "enum": ["cover", "content", "section", "split", "chart", "grid", "blank"],
                            "description": "Optional per-slide layout; omitted → chosen from the slide's content. \
                                            `grid` renders the slide's bullets as numbered cards (agenda/overview look)."
                        },
                        "image": {
                            "type": "object",
                            "description": "Optional visual for this slide, sourced by ONE of (checked in order): \
                                            `query` — a few search words; a matching, commercially-licensed photo \
                                            is fetched keylessly and embedded (the easiest way to illustrate a \
                                            slide, no image key needed); `url` — a direct image link; or `path` — \
                                            a local PNG/JPEG you already have. Add `alt_text` for accessibility. A \
                                            source that can't be resolved is skipped and reported in `image_notes`, \
                                            never fatal.",
                            "properties": {
                                "query": {"type": "string", "description": "Search words, e.g. 'stanford campus autumn'. A relevant photo is downloaded and embedded automatically."},
                                "url": {"type": "string", "description": "Direct link to an image to download and embed."},
                                "path": {"type": "string", "description": "Local PNG/JPEG path (e.g. one you generated with image_generation or extracted via read_document)."},
                                "alt_text": {"type": "string"}
                            }
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

/// The `format` + `path` needed to place the output, read before the full spec
/// so an edit can locate its manifest. Extra request fields are ignored.
#[derive(serde::Deserialize)]
struct DocTarget {
    format: DocFormat,
    path: String,
}

#[async_trait]
impl ToolExecutor for CreateDocumentTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let target_spec: DocTarget = match serde_json::from_value(args.clone()) {
            Ok(target) => target,
            Err(error) => {
                return Ok(tool_error_json(format!(
                    "invalid create_document spec: {error}"
                )));
            }
        };

        // Bug #10: a relative path lands in the ARTIFACTS area, not the
        // deacon's launch cwd — the prompt already points the model there,
        // but steering is not enforcement. Absolute paths are honored as-is
        // (still jail-checked); contexts without an artifacts dir (CLI/repl,
        // tests) keep the old cwd-relative behavior. `has_root` matters on
        // Windows: `\x` is not "absolute" (no drive) but Path::join would let
        // it REPLACE the artifacts base — rooted paths never join.
        let p = Path::new(&target_spec.path);
        let target = match (&ctx.artifacts_dir, p.is_relative() && !p.has_root()) {
            (Some(artifacts), true) => artifacts
                .join(artifact_relative_path(
                    target_spec.format,
                    &target_spec.path,
                ))
                .display()
                .to_string(),
            _ => target_spec.path.clone(),
        };
        let resolved = match ctx.resolve(&target) {
            Ok(resolved) => resolved,
            Err(error) => return Ok(tool_error_json(error.to_string())),
        };
        let manifest_file = edit::manifest_path(&resolved);
        let is_edit = args.get("operation").and_then(Value::as_str) == Some("edit");
        let want_preview = args.get("preview").and_then(Value::as_bool) == Some(true);

        // For an edit, the content comes from the saved manifest with the
        // request's `patch` merged in; for a create, the request itself is the
        // content. Either way `content` is the pure spec we render and persist.
        let content = if is_edit {
            let bytes = match tokio::fs::read(&manifest_file).await {
                Ok(bytes) => bytes,
                Err(_) => {
                    return Ok(tool_error_json(format!(
                        "no manifest at {} — `operation: \"edit\"` only works on files \
                         create_document made. For a third-party file, read_document it and \
                         create a new one.",
                        manifest_file.display()
                    )));
                }
            };
            let mut manifest: Value = match serde_json::from_slice(&bytes) {
                Ok(value) => value,
                Err(error) => {
                    return Ok(tool_error_json(format!(
                        "manifest {} is corrupt: {error}",
                        manifest_file.display()
                    )));
                }
            };
            if let Some(patch) = args.get("patch") {
                edit::merge_patch(&mut manifest, patch);
            }
            edit::content_only(manifest)
        } else {
            edit::content_only(args)
        };

        let mut spec: DocumentSpec = match serde_json::from_value(content.clone()) {
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
        // Slides (pptx) embed images directly; sections (pdf report) get an
        // inline data URI. Each is a no-op for the other's format, so only one
        // runs. Both share the soft-note contract.
        let mut image_notes =
            match images::hydrate_slides(&mut spec, ctx, resolved.parent()).await {
                Ok(notes) => notes,
                Err(message) => return Ok(tool_error_json(message)),
            };
        match images::hydrate_sections(&mut spec, ctx, resolved.parent()).await {
            Ok(notes) => image_notes.extend(notes),
            Err(message) => return Ok(tool_error_json(message)),
        }

        if let Some(parent) = resolved.parent()
            && let Err(error) = tokio::fs::create_dir_all(parent).await
        {
            return Ok(tool_error_json(format!(
                "cannot create parent directory {}: {error}",
                parent.display()
            )));
        }

        // PDF/PPTX render through the sidecar (themed HTML→PDF, PptxGenJS) when
        // it is available, else the native Rust writers; DOCX/XLSX are always
        // native. Synthesis is either a subprocess (async) or CPU-bound work
        // offloaded to spawn_blocking — never on the async runtime directly.
        let format = spec.format;
        // Capture what the (optional) preview needs before `synthesize` consumes
        // the spec: the report path re-renders the same themed HTML.
        let preview_input = want_preview.then(|| {
            let resolved_theme = theme::resolve(spec.theme.as_ref(), theme_seed(&spec));
            (spec.clone(), resolved_theme)
        });
        let bytes = match synthesize(spec).await {
            Ok(bytes) => bytes,
            Err(message) => return Ok(tool_error_json(message)),
        };

        if let Err(error) = tokio::fs::write(&resolved, &bytes).await {
            return Ok(tool_error_json(format!(
                "cannot write {}: {error}",
                resolved.display()
            )));
        }

        // Persist the manifest so this file can be edited later. The document is
        // already on disk; a manifest failure is surfaced, not fatal.
        let manifest_result = tokio::fs::write(
            &manifest_file,
            serde_json::to_vec_pretty(&content).unwrap_or_default(),
        )
        .await;

        let mut result = json!({
            "created": resolved.display().to_string(),
            "folder": resolved.parent().map(|path| path.display().to_string()),
            "format": format.as_str(),
            "operation": if is_edit { "edit" } else { "create" },
            "bytes": bytes.len(),
        });
        match manifest_result {
            Ok(()) => {
                result["manifest"] = json!(manifest_file.display().to_string());
            }
            Err(error) => {
                result["manifest_error"] = json!(format!(
                    "document written but manifest {} failed ({error}); this file cannot be edited in place",
                    manifest_file.display()
                ));
            }
        }

        // Any images that couldn't be sourced (a query that matched nothing, a
        // dead url) are reported so the model can adjust — the deck shipped anyway.
        if !image_notes.is_empty() {
            result["image_notes"] = json!(image_notes);
        }

        // Optional background preview image for a vision QA pass. Headless — no
        // window, no focus steal — so it is safe while the user is on the
        // machine. Never fatal: the document is already written.
        if let Some((preview_spec, preview_theme)) = preview_input {
            let outcome = match format {
                DocFormat::Pdf => {
                    preview::preview_pdf(&resolved, &preview_spec, &preview_theme).await
                }
                DocFormat::Pptx => preview::preview_pptx(&resolved).await,
                _ => Err("preview is only produced for pdf and pptx".to_owned()),
            };
            match outcome {
                Ok(path) => result["preview"] = json!(path.display().to_string()),
                Err(note) => result["preview_note"] = json!(note),
            }
        }
        Ok(tool_result_json(result))
    }
}

/// A bare deck filename is ambiguous in a shared artifact root. Put it in a
/// deterministic, human-readable folder while preserving explicitly supplied
/// subfolders for callers that already organize their output.
fn artifact_relative_path(format: DocFormat, path_str: &str) -> PathBuf {
    let path = Path::new(path_str);
    if format != DocFormat::Pptx || path.components().count() != 1 {
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

/// Produce the document bytes: PDF/PPTX prefer the themed renderer sidecar and
/// fall back to the native writers when it is absent; DOCX/XLSX are native.
async fn synthesize(spec: DocumentSpec) -> Result<Vec<u8>, String> {
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
fn theme_seed(spec: &DocumentSpec) -> &str {
    spec.title
        .as_deref()
        .or_else(|| spec.sections.first().and_then(|s| s.heading.as_deref()))
        .or_else(|| spec.slides.first().map(|s| s.title.as_str()))
        .or_else(|| spec.sheets.first().map(|s| s.name.as_str()))
        .unwrap_or("regent")
}

/// Registers `create_document` on the catalog.
pub fn register_create_document_tool(catalog: &mut ToolCatalog) -> Result<(), RegentError> {
    catalog.register(definition(), Arc::new(CreateDocumentTool))
}

#[cfg(test)]
#[path = "create_document_tests.rs"]
mod tests;

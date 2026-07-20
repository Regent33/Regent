//! `create_document` — generate PDF / Word / PowerPoint / Excel files natively
//! in-process, the write-side twin of `read_document`. Same motivation: with no
//! document tool the model fell back to `python3 -c` one-liners that hang on
//! Windows. One flat content spec (`model::DocumentSpec`) drives all four
//! writers; `format` picks which of `sections` / `slides` / `sheets` is
//! authoritative. This module is the executor: it resolves + jails the output
//! path, loads content (create or edit), hydrates images, then hands off to
//! `synth` for the bytes and writes them once. Schema lives in `schema`, byte
//! synthesis in `synth`, path placement in `paths`.

mod deck;
mod docx;
mod edit;
mod html;
mod images;
mod model;
mod paths;
mod pdf;
mod pptx;
mod pptx_scaffold;
mod pptx_shapes;
mod pptx_slide;
mod pptx_xml;
mod preview;
mod renderer;
mod schema;
mod synth;
mod theme;
mod xlsx;

use crate::ToolCatalog;
use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use model::{DocFormat, DocumentSpec};
use regent_kernel::{RegentError, tool_error_json, tool_result_json};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;

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
                .join(paths::artifact_relative_path(
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

        let format = spec.format;
        // Capture what the (optional) preview needs before `synthesize` consumes
        // the spec: the report path re-renders the same themed HTML.
        let preview_input = want_preview.then(|| {
            let resolved_theme = theme::resolve(spec.theme.as_ref(), synth::theme_seed(&spec));
            (spec.clone(), resolved_theme)
        });
        let bytes = match synth::synthesize(spec).await {
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

/// Registers `create_document` on the catalog.
pub fn register_create_document_tool(catalog: &mut ToolCatalog) -> Result<(), RegentError> {
    catalog.register(schema::definition(), Arc::new(CreateDocumentTool))
}

#[cfg(test)]
#[path = "tests/support.rs"]
mod tests_support;

#[cfg(test)]
#[path = "tests/images.rs"]
mod image_tests;

#[cfg(test)]
#[path = "tests/edit.rs"]
mod edit_tests;

#[cfg(test)]
#[path = "tests/round_trip.rs"]
mod tests;

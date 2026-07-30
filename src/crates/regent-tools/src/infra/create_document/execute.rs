//! `create_document` executor: resolve, validate, hydrate, synthesize, write.

use super::model::{DocFormat, DocumentSpec};
use super::{edit, images, paths, preview, synth, theme};
use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, tool_error_json, tool_result_json};
use serde_json::{Value, json};
use std::path::Path;

pub(super) struct CreateDocumentTool;

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
        let path = Path::new(&target_spec.path);
        let target = match (&ctx.artifacts_dir, path.is_relative() && !path.has_root()) {
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
        let mut image_notes = match images::hydrate_slides(&mut spec, ctx, resolved.parent()).await
        {
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
        let preview_input = want_preview.then(|| {
            let resolved_theme = theme::resolve(spec.theme.as_ref(), synth::theme_seed(&spec));
            (spec.clone(), resolved_theme)
        });
        let (bytes, render_note) = match synth::synthesize(spec).await {
            Ok(result) => result,
            Err(message) => return Ok(tool_error_json(message)),
        };
        if let Err(error) = tokio::fs::write(&resolved, &bytes).await {
            return Ok(tool_error_json(format!(
                "cannot write {}: {error}",
                resolved.display()
            )));
        }
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
            Ok(()) => result["manifest"] = json!(manifest_file.display().to_string()),
            Err(error) => {
                result["manifest_error"] = json!(format!(
                    "document written but manifest {} failed ({error}); this file cannot be edited in place",
                    manifest_file.display()
                ));
            }
        }
        if !image_notes.is_empty() {
            result["image_notes"] = json!(image_notes);
        }
        if let Some(note) = render_note {
            result["render_note"] = json!(note);
        }
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

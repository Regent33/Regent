//! Output-path routing (artifacts dir, presentation folders), the
//! edit-via-manifest flow, and the optional renderer/preview paths.

use super::tests_support::create;
use super::*;
use crate::domain::contracts::DenyAll;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

// Bug #10: with an artifacts dir set (the deacon's context), a RELATIVE path
// lands under artifacts — not the launch cwd; an absolute path is honored.
#[tokio::test]
async fn relative_paths_land_in_the_artifacts_dir_when_one_is_set() {
    let cwd = TempDir::new().unwrap();
    let artifacts = TempDir::new().unwrap();
    let ctx = ToolContext::new(cwd.path().to_path_buf(), Arc::new(DenyAll))
        .with_artifacts_dir(artifacts.path().to_path_buf());

    let args = json!({
        "format": "pdf", "path": "out/report.pdf", "title": "t",
        "sections": [{"paragraphs": ["hello"]}]
    });
    let out = CreateDocumentTool.execute(args, &ctx).await.unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    let created = PathBuf::from(v["created"].as_str().unwrap());
    assert!(
        created.starts_with(artifacts.path()),
        "relative output must land under artifacts, got {created:?}"
    );
    assert!(created.exists());

    // Absolute path: honored as-is, artifacts dir ignored.
    let explicit = cwd.path().join("explicit.pdf");
    let args = json!({
        "format": "pdf", "path": explicit.to_str().unwrap(), "title": "t",
        "sections": [{"paragraphs": ["hello"]}]
    });
    let out = CreateDocumentTool.execute(args, &ctx).await.unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(PathBuf::from(v["created"].as_str().unwrap()), explicit);
}

#[tokio::test]
async fn bare_pptx_names_get_a_presentation_folder() {
    let cwd = TempDir::new().unwrap();
    let artifacts = TempDir::new().unwrap();
    let ctx = ToolContext::new(cwd.path().to_path_buf(), Arc::new(DenyAll))
        .with_artifacts_dir(artifacts.path().to_path_buf());

    let out = CreateDocumentTool
        .execute(
            json!({
                "format": "pptx",
                "path": "AI_Roadmap_Stanford_Certificate.pptx",
                "slides": [{"title": "Roadmap"}]
            }),
            &ctx,
        )
        .await
        .unwrap();
    let value: Value = serde_json::from_str(&out).unwrap();
    let created = PathBuf::from(value["created"].as_str().unwrap());
    assert_eq!(
        created.parent().unwrap().file_name().unwrap(),
        "ai-roadmap-stanford-certificate"
    );
    assert!(created.exists());
}

#[tokio::test]
async fn edit_applies_a_merge_patch_over_the_saved_spec() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = ToolContext::new(dir.path().to_path_buf(), Arc::new(DenyAll));

    // Create a PDF with an original title and body.
    let out = CreateDocumentTool
        .execute(
            json!({
                "format": "pdf", "path": "report.pdf",
                "title": "OriginalTitle",
                "sections": [{"paragraphs": ["ManifestBodyToken."]}]
            }),
            &ctx,
        )
        .await
        .unwrap();
    let created: Value = serde_json::from_str(&out).unwrap();
    assert!(created.get("error").is_none(), "create failed: {created}");
    let manifest = PathBuf::from(created["manifest"].as_str().unwrap());
    assert!(manifest.exists(), "manifest not written beside the file");

    // Edit: patch only the title; the body must come from the manifest.
    let out = CreateDocumentTool
        .execute(
            json!({
                "format": "pdf", "path": "report.pdf",
                "operation": "edit",
                "patch": {"title": "RevisedTitle"}
            }),
            &ctx,
        )
        .await
        .unwrap();
    let edited: Value = serde_json::from_str(&out).unwrap();
    assert!(edited.get("error").is_none(), "edit failed: {edited}");
    assert_eq!(edited["operation"], "edit");

    let path = PathBuf::from(edited["created"].as_str().unwrap());
    let squashed: String = pdf_extract::extract_text(&path)
        .unwrap()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    assert!(
        squashed.contains("RevisedTitle"),
        "patch not applied: {squashed}"
    );
    assert!(
        !squashed.contains("OriginalTitle"),
        "old title survived: {squashed}"
    );
    assert!(
        squashed.contains("ManifestBodyToken"),
        "body from the manifest was lost on edit: {squashed}"
    );
}

#[tokio::test]
async fn edit_without_a_manifest_is_a_clear_error() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = ToolContext::new(dir.path().to_path_buf(), Arc::new(DenyAll));
    let out = CreateDocumentTool
        .execute(
            json!({
                "format": "pdf", "path": "never-made.pdf",
                "operation": "edit", "patch": {"title": "X"}
            }),
            &ctx,
        )
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert!(
        v["error"].as_str().unwrap_or_default().contains("manifest"),
        "expected a manifest-missing error, got: {v}"
    );
}

// When a renderer is available (dev box with bun + a browser), the executor
// takes the themed HTML→PDF path with the model's custom palette. Skips cleanly
// where none exists, so the Rust CI job (no bun/browser) stays green.
#[tokio::test]
async fn renderer_path_produces_a_themed_pdf_when_available() {
    if super::renderer::find_renderer().is_none() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = create(
        &dir,
        json!({
            "format": "pdf", "path": "themed.pdf",
            "title": "RendererThemedReport",
            "theme": {"accent": "FF3366"},
            "sections": [{"heading": "OverviewSection", "paragraphs": ["Rendered via headless Chromium."]}]
        }),
    )
    .await;
    let text = pdf_extract::extract_text(&path).unwrap();
    let squashed: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        squashed.contains("RendererThemedReport"),
        "themed PDF missing title; got: {text}"
    );
    assert!(
        squashed.contains("OverviewSection"),
        "themed PDF missing heading"
    );
}

// preview:true renders a background <file>.preview.png (headless) for the vision
// loop. Verifiable here for the PDF/report path; skips without a renderer.
#[tokio::test]
async fn preview_true_produces_a_background_png_for_reports() {
    if super::renderer::find_renderer().is_none() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let ctx = ToolContext::new(dir.path().to_path_buf(), Arc::new(DenyAll));
    let out = CreateDocumentTool
        .execute(
            json!({
                "format": "pdf", "path": "preview-report.pdf",
                "title": "PreviewMe", "preview": true,
                "sections": [{"heading": "Alpha", "paragraphs": ["Body text."]}]
            }),
            &ctx,
        )
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert!(v.get("error").is_none(), "tool error: {v}");
    let preview = v["preview"]
        .as_str()
        .expect("preview path missing in result");
    let png = std::fs::read(preview).unwrap();
    assert_eq!(&png[..4], &[0x89, 0x50, 0x4e, 0x47], "preview is not a PNG");
}

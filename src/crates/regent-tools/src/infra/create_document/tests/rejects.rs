//! Executor-level rejection tests: a malformed request must fail with a
//! sentence BEFORE any file is written, so the model can correct itself and the
//! user never gets a broken document. Complements the format-mismatch test in
//! tests/round_trip.rs; the layout-value unit tests live in validate.rs.

use super::tests_support::create;
use super::*;
use crate::domain::contracts::DenyAll;
use std::sync::Arc;

/// Incident 2026-07-23: on a voice call the model answered "Ferrari vs
/// Lamborghini" by calling create_document with diagram-only PPTX slides —
/// `layout: "compare"` plus `items: [{name, points}]`. serde silently dropped
/// the unknown `items`, the bogus layout fell back to a divider, and the user
/// got title-only slides. `deny_unknown_fields` on `Slide` must now reject the
/// unknown field with a sentence, BEFORE any file is written; the inline
/// VISUAL_EXPLAINER compare diagram is the real path.
#[tokio::test]
async fn diagram_shaped_pptx_slides_are_rejected_before_writing() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = ToolContext::new(dir.path().to_path_buf(), Arc::new(DenyAll));
    let out = CreateDocumentTool
        .execute(
            json!({
                "format": "pptx", "path": "compare.pptx",
                "slides": [{
                    "title": "Ferrari vs Lamborghini",
                    "layout": "compare",
                    "items": [
                        {"name": "Ferrari", "points": ["F1 heritage", "Higher top speed"]},
                        {"name": "Lamborghini", "points": ["V12 drama", "Bolder styling"]}
                    ]
                }]
            }),
            &ctx,
        )
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert!(
        v["error"].as_str().unwrap_or_default().contains("items"),
        "the unknown slide field must be rejected, got: {v}"
    );
    assert!(v.get("created").is_none(), "no file may be reported: {v}");
    assert!(
        !dir.path().join("compare.pptx").exists(),
        "no deck may be written for a rejected spec"
    );
}

/// A bogus `layout` on its own (no unknown fields to trip `deny_unknown_fields`)
/// must still fail validation with a descriptive, layout-naming error before
/// synthesis — the second half of the same incident.
#[tokio::test]
async fn unknown_pptx_layout_alone_is_rejected_before_writing() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = ToolContext::new(dir.path().to_path_buf(), Arc::new(DenyAll));
    let out = CreateDocumentTool
        .execute(
            json!({
                "format": "pptx", "path": "bad.pptx",
                "slides": [{"title": "Compare", "layout": "compare", "bullets": ["a", "b"]}]
            }),
            &ctx,
        )
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    let err = v["error"].as_str().unwrap_or_default();
    assert!(
        err.contains("layout") && err.contains("compare"),
        "expected a descriptive layout error, got: {v}"
    );
    assert!(v.get("created").is_none(), "no file may be reported: {v}");
    assert!(
        !dir.path().join("bad.pptx").exists(),
        "no deck may be written for a rejected layout"
    );
}

/// The valid path is untouched: a well-formed deck with a real layout renders.
#[tokio::test]
async fn valid_pptx_with_a_known_layout_still_builds() {
    let dir = tempfile::tempdir().unwrap();
    let path = create(
        &dir,
        json!({
            "format": "pptx", "path": "ok.pptx",
            "slides": [
                {"title": "Cover"},
                {"title": "Three points", "layout": "grid", "bullets": ["One", "Two", "Three"]}
            ]
        }),
    )
    .await;
    assert!(path.exists(), "a valid deck must be written");
}

//! Image-sourcing tests: embedded slide pictures and the soft-failure
//! `image_notes` contract for slides (PPTX) and sections (PDF).

use super::model::EmbeddedSlideImage;
use super::tests_support::{zip_entry, zip_names};
use super::*;
use crate::domain::contracts::DenyAll;
use std::sync::Arc;

#[test]
fn native_pptx_embeds_an_optional_slide_image() {
    let dir = tempfile::tempdir().unwrap();
    let image_path = dir.path().join("roadmap.png");
    image::RgbImage::from_pixel(8, 4, image::Rgb([16, 161, 155]))
        .save(&image_path)
        .unwrap();

    let mut spec: DocumentSpec = serde_json::from_value(json!({
        "format": "pptx",
        "slides": [{
            "title": "A visual claim",
            "subtitle": "Evidence, not decoration",
            "bullets": ["One concise point"]
        }]
    }))
    .unwrap();
    // Attach the image as the executor's hydrate step would before native build.
    spec.slides[0].embedded_image = Some(EmbeddedSlideImage {
        bytes: std::fs::read(&image_path).unwrap(),
        extension: "png",
        content_type: "image/png",
        width: 8,
        height: 4,
        alt_text: "Teal roadmap illustration".to_owned(),
    });
    let bytes = super::pptx::build(&spec).unwrap();
    let path = dir.path().join("visual-deck.pptx");
    std::fs::write(&path, &bytes).unwrap();

    let names = zip_names(&path);
    assert!(
        names.iter().any(|name| name == "ppt/media/image1.png"),
        "PPTX is missing the embedded image: {names:?}"
    );
    let slide = zip_entry(&path, "ppt/slides/slide1.xml");
    assert!(slide.contains("<p:pic>"), "slide has no picture shape");
    assert!(
        slide.contains("Teal roadmap illustration"),
        "image alt text is missing"
    );
    let rels = zip_entry(&path, "ppt/slides/_rels/slide1.xml.rels");
    assert!(
        rels.contains("relationships/image"),
        "slide has no image relationship"
    );
}

// A slide image with no obtainable source must NOT sink the deck: the file is
// still created and the miss is reported in `image_notes` for the model to see.
#[tokio::test]
async fn slide_image_without_a_source_notes_and_still_builds() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = ToolContext::new(dir.path().to_path_buf(), Arc::new(DenyAll));
    let out = CreateDocumentTool
        .execute(
            json!({
                "format": "pptx", "path": "notes-deck.pptx",
                "slides": [{"title": "Sourceless", "image": {}}]
            }),
            &ctx,
        )
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert!(v.get("error").is_none(), "tool errored: {v}");
    assert!(v["bytes"].as_u64().unwrap() > 0, "deck not built: {v}");
    let notes = v["image_notes"].as_array().expect("image_notes missing");
    assert!(
        notes.iter().any(|n| n.as_str().unwrap_or("").contains("slide 1")),
        "expected a note for the unsourced slide 1 image: {v}"
    );
}

// A local-path slide image is read and embedded through the executor's hydrate
// step, with no note (the happy path for the `path` source).
#[tokio::test]
async fn slide_image_from_a_local_path_embeds_via_the_executor() {
    let dir = tempfile::tempdir().unwrap();
    image::RgbImage::from_pixel(8, 4, image::Rgb([16, 161, 155]))
        .save(dir.path().join("pic.png"))
        .unwrap();
    let ctx = ToolContext::new(dir.path().to_path_buf(), Arc::new(DenyAll));
    let out = CreateDocumentTool
        .execute(
            json!({
                "format": "pptx", "path": "local-deck.pptx",
                "slides": [{"title": "Has a picture", "image": {"path": "pic.png"}}]
            }),
            &ctx,
        )
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert!(v.get("error").is_none(), "tool errored: {v}");
    assert!(v["bytes"].as_u64().unwrap() > 0, "deck not built: {v}");
    assert!(
        v.get("image_notes").is_none(),
        "a readable local image should produce no notes: {v}"
    );
}

// A PDF section can carry an image from a local path; it hydrates without a note
// (whether or not a renderer is present to actually draw it).
#[tokio::test]
async fn section_image_from_a_local_path_hydrates_without_a_note() {
    let dir = tempfile::tempdir().unwrap();
    image::RgbImage::from_pixel(6, 4, image::Rgb([20, 40, 80]))
        .save(dir.path().join("fig.png"))
        .unwrap();
    let ctx = ToolContext::new(dir.path().to_path_buf(), Arc::new(DenyAll));
    let out = CreateDocumentTool
        .execute(
            json!({
                "format": "pdf", "path": "illustrated.pdf", "title": "Illustrated",
                "sections": [{"heading": "Figure", "paragraphs": ["body"], "image": {"path": "fig.png"}}]
            }),
            &ctx,
        )
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert!(v.get("error").is_none(), "tool errored: {v}");
    assert!(v["bytes"].as_u64().unwrap() > 0, "pdf not built: {v}");
    assert!(
        v.get("image_notes").is_none(),
        "a readable section image should produce no notes: {v}"
    );
}

// A sourceless section image is noted, and the PDF is still produced.
#[tokio::test]
async fn section_image_without_a_source_notes_and_still_builds() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = ToolContext::new(dir.path().to_path_buf(), Arc::new(DenyAll));
    let out = CreateDocumentTool
        .execute(
            json!({
                "format": "pdf", "path": "noimg.pdf", "title": "T",
                "sections": [{"heading": "H", "paragraphs": ["p"], "image": {}}]
            }),
            &ctx,
        )
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert!(v.get("error").is_none(), "tool errored: {v}");
    assert!(v["bytes"].as_u64().unwrap() > 0, "pdf not built: {v}");
    let notes = v["image_notes"].as_array().expect("image_notes missing");
    assert!(
        notes.iter().any(|n| n.as_str().unwrap_or("").contains("section 1")),
        "expected a note for the unsourced section image: {v}"
    );
}

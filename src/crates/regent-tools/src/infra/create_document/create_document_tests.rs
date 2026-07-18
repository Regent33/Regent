//! Round-trip tests: every format is generated through the real tool executor
//! (jailed `ToolContext`, tempdir output) and then read back — xlsx via
//! calamine, pdf via pdf_extract, docx/pptx by cracking the zip and inspecting
//! the parts. If a file we emit can't be re-opened, the test fails.

use super::*;
use super::model::EmbeddedSlideImage;
use crate::domain::contracts::DenyAll;
use std::io::Read;
use std::path::PathBuf;
use tempfile::TempDir;

/// Runs `create_document` with `args` and returns the created file path.
async fn create(dir: &TempDir, args: Value) -> PathBuf {
    let ctx = ToolContext::new(dir.path().to_path_buf(), Arc::new(DenyAll));
    let out = CreateDocumentTool.execute(args, &ctx).await.unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert!(v.get("error").is_none(), "tool returned an error: {v}");
    assert!(v["bytes"].as_u64().unwrap() > 0, "empty file: {v}");
    PathBuf::from(v["created"].as_str().unwrap())
}

/// Reads one entry out of a ZIP-backed document into a string.
fn zip_entry(path: &PathBuf, entry: &str) -> String {
    let file = std::fs::File::open(path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let mut out = String::new();
    archive
        .by_name(entry)
        .unwrap_or_else(|_| panic!("missing zip part {entry}"))
        .read_to_string(&mut out)
        .unwrap();
    out
}

fn zip_names(path: &PathBuf) -> Vec<String> {
    let file = std::fs::File::open(path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().to_owned())
        .collect()
}

#[tokio::test]
async fn xlsx_round_trips_through_calamine() {
    use calamine::{Data, Reader};
    let dir = tempfile::tempdir().unwrap();
    let path = create(
        &dir,
        json!({
            "format": "xlsx", "path": "book.xlsx",
            "sheets": [{
                "name": "Numbers",
                "header": true,
                "rows": [["Item", "Qty"], ["Widgets", 42], ["Gadgets", 7.5]]
            }]
        }),
    )
    .await;

    let mut wb = calamine::open_workbook_auto(&path).unwrap();
    assert_eq!(wb.sheet_names(), &["Numbers"]);
    let range = wb.worksheet_range("Numbers").unwrap();
    assert_eq!(range.get((0, 0)), Some(&Data::String("Item".into())));
    assert_eq!(range.get((1, 0)), Some(&Data::String("Widgets".into())));
    // Numbers must survive as numbers, not text.
    assert_eq!(range.get((1, 1)), Some(&Data::Float(42.0)));
    assert_eq!(range.get((2, 1)), Some(&Data::Float(7.5)));
}

// The native lopdf writer, tested directly — it is the fallback used when the
// renderer sidecar is absent (e.g. CI without bun/a browser). The renderer
// (themed HTML→PDF) path is exercised by the executor tests when one is present.
#[test]
fn native_pdf_text_is_extractable() {
    let spec: DocumentSpec = serde_json::from_value(json!({
        "format": "pdf",
        "title": "ZephyrTitle",
        "sections": [{
            "heading": "IntroHeading",
            "paragraphs": ["The word Photosynthesis appears in the body."],
            "bullets": ["FirstBullet", "SecondBullet"]
        }]
    }))
    .unwrap();
    let bytes = super::pdf::build(&spec).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.pdf");
    std::fs::write(&path, &bytes).unwrap();

    let text = pdf_extract::extract_text(&path).unwrap();
    let squashed: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    for token in [
        "ZephyrTitle",
        "IntroHeading",
        "Photosynthesis",
        "FirstBullet",
    ] {
        assert!(squashed.contains(token), "PDF missing {token}; got: {text}");
    }
    // The bullet glyph must survive the WinAnsi encoding round trip — raw
    // UTF-8 in the text stream renders it as "â€¢" mojibake.
    assert!(squashed.contains('\u{2022}'), "bullet glyph lost: {text}");
    assert!(!squashed.contains("â€¢"), "mojibake bullet: {text}");
}

#[tokio::test]
async fn docx_parts_and_text_present() {
    let dir = tempfile::tempdir().unwrap();
    let path = create(
        &dir,
        json!({
            "format": "docx", "path": "doc.docx",
            "title": "MyDocTitle",
            "sections": [{
                "heading": "SectionOne",
                "paragraphs": ["A plain paragraph here."],
                "bullets": ["BulletAlpha", "BulletBeta"]
            }]
        }),
    )
    .await;

    let names = zip_names(&path);
    for part in ["[Content_Types].xml", "_rels/.rels", "word/document.xml"] {
        assert!(
            names.iter().any(|n| n == part),
            "docx missing {part}: {names:?}"
        );
    }
    let doc = zip_entry(&path, "word/document.xml");
    for token in ["MyDocTitle", "SectionOne", "BulletAlpha"] {
        assert!(doc.contains(token), "document.xml missing {token}");
    }
}

#[test]
fn native_pptx_parts_and_slide_text_present() {
    let spec: DocumentSpec = serde_json::from_value(json!({
        "format": "pptx",
        "slides": [
            {"title": "SlideOneTitle", "bullets": ["PointA", "PointB"], "notes": "SpeakerNoteHere"},
            {"title": "SlideTwoTitle", "bullets": ["PointC"]}
        ]
    }))
    .unwrap();
    let bytes = super::pptx::build(&spec).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("deck.pptx");
    std::fs::write(&path, &bytes).unwrap();

    let names = zip_names(&path);
    for part in [
        "[Content_Types].xml",
        "_rels/.rels",
        "ppt/presentation.xml",
        "ppt/slides/slide1.xml",
        "ppt/slides/slide2.xml",
        "ppt/slideMasters/slideMaster1.xml",
        "ppt/theme/theme1.xml",
        "ppt/notesSlides/notesSlide1.xml",
        "ppt/notesMasters/notesMaster1.xml",
    ] {
        assert!(
            names.iter().any(|n| n == part),
            "pptx missing {part}: {names:?}"
        );
    }
    let slide1 = zip_entry(&path, "ppt/slides/slide1.xml");
    assert!(slide1.contains("SlideOneTitle"), "slide1 missing title");
    assert!(slide1.contains("PointA"), "slide1 missing bullet");
    assert!(
        slide1.matches("<a:solidFill>").count() >= 3,
        "slides need an intentional visual system, not plain text boxes"
    );
    assert!(
        slide1.contains("171C2C") && slide1.contains("00A19B"),
        "deck theme colors are missing"
    );
    let notes1 = zip_entry(&path, "ppt/notesSlides/notesSlide1.xml");
    assert!(
        notes1.contains("SpeakerNoteHere"),
        "notes1 missing note text"
    );

    // EVERY slide — with or without notes — must carry a rels file relating
    // it to its layout, or PowerPoint reports the package corrupt
    // (found the hard way, via COM, 2026-07-16).
    for n in [1, 2] {
        let rels = zip_entry(&path, &format!("ppt/slides/_rels/slide{n}.xml.rels"));
        assert!(
            rels.contains("relationships/slideLayout"),
            "slide{n} rels missing the layout relationship: {rels}"
        );
    }
    let rels1 = zip_entry(&path, "ppt/slides/_rels/slide1.xml.rels");
    assert!(
        rels1.contains("relationships/notesSlide"),
        "slide1 links notes"
    );
}

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

#[tokio::test]
async fn wrong_spec_for_format_is_a_clear_error() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = ToolContext::new(dir.path().to_path_buf(), Arc::new(DenyAll));
    // pptx with no slides — the driving spec is absent.
    let out = CreateDocumentTool
        .execute(json!({"format": "pptx", "path": "x.pptx"}), &ctx)
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert!(
        v["error"].as_str().unwrap_or_default().contains("slides"),
        "expected a descriptive slides error, got: {v}"
    );
}

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

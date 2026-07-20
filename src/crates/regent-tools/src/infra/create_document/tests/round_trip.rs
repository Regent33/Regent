//! Round-trip tests: every format is generated through the real tool executor
//! (jailed `ToolContext`, tempdir output) and then read back — xlsx via
//! calamine, pdf via pdf_extract, docx/pptx by cracking the zip and inspecting
//! the parts. If a file we emit can't be re-opened, the test fails.
//! (Image-sourcing tests: tests/images.rs; edit/path/renderer tests:
//! tests/edit.rs; shared helpers: tests/support.rs.)

use super::tests_support::{create, zip_entry, zip_names};
use super::*;
use crate::domain::contracts::DenyAll;
use std::sync::Arc;

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

/// Word used to ignore the theme entirely — every document was the same black
/// Calibri. It must now carry the document's own generated palette and fonts,
/// and two documents on different subjects must not come out identical.
#[tokio::test]
async fn docx_carries_a_per_document_theme() {
    async fn document_xml(title: &str) -> String {
        let dir = tempfile::tempdir().unwrap();
        let path = create(
            &dir,
            json!({
                "format": "docx", "path": "doc.docx", "title": title,
                "sections": [{"heading": "Heading", "paragraphs": ["Body text."]}]
            }),
        )
        .await;
        zip_entry(&path, "word/document.xml")
    }

    let quantum = document_xml("Quantum Computing Review").await;
    assert!(
        quantum.contains("<w:color w:val=") && quantum.contains("<w:rFonts"),
        "docx carries no theme color or font: {quantum}"
    );
    assert_ne!(
        quantum,
        document_xml("Mediterranean Cookery").await,
        "two unrelated documents rendered identically"
    );
}

/// Excel's header row must pick up the accent fill + freeze pane, so a workbook
/// looks handed-over rather than dumped.
#[tokio::test]
async fn xlsx_header_is_themed_and_frozen() {
    let dir = tempfile::tempdir().unwrap();
    let path = create(
        &dir,
        json!({
            "format": "xlsx", "path": "book.xlsx",
            "sheets": [{"name": "Data", "header": true, "rows": [["Item"], ["Widget"]]}]
        }),
    )
    .await;
    let sheet = zip_entry(&path, "xl/worksheets/sheet1.xml");
    assert!(
        sheet.contains("<pane"),
        "header row was not frozen: {sheet}"
    );
    assert!(sheet.contains("<col "), "columns were not width-fitted");
    let styles = zip_entry(&path, "xl/styles.xml");
    assert!(
        styles.contains("<patternFill patternType=\"solid\""),
        "header row has no accent fill: {styles}"
    );
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

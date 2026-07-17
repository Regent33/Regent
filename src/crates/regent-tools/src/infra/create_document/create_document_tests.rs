//! Round-trip tests: every format is generated through the real tool executor
//! (jailed `ToolContext`, tempdir output) and then read back — xlsx via
//! calamine, pdf via pdf_extract, docx/pptx by cracking the zip and inspecting
//! the parts. If a file we emit can't be re-opened, the test fails.

use super::*;
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

#[tokio::test]
async fn pdf_text_is_extractable() {
    let dir = tempfile::tempdir().unwrap();
    let path = create(
        &dir,
        json!({
            "format": "pdf", "path": "report.pdf",
            "title": "ZephyrTitle",
            "sections": [{
                "heading": "IntroHeading",
                "paragraphs": ["The word Photosynthesis appears in the body."],
                "bullets": ["FirstBullet", "SecondBullet"]
            }]
        }),
    )
    .await;

    let text = pdf_extract::extract_text(&path).unwrap();
    let squashed: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    for token in ["ZephyrTitle", "IntroHeading", "Photosynthesis", "FirstBullet"] {
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
        assert!(names.iter().any(|n| n == part), "docx missing {part}: {names:?}");
    }
    let doc = zip_entry(&path, "word/document.xml");
    for token in ["MyDocTitle", "SectionOne", "BulletAlpha"] {
        assert!(doc.contains(token), "document.xml missing {token}");
    }
}

#[tokio::test]
async fn pptx_parts_and_slide_text_present() {
    let dir = tempfile::tempdir().unwrap();
    let path = create(
        &dir,
        json!({
            "format": "pptx", "path": "deck.pptx",
            "slides": [
                {"title": "SlideOneTitle", "bullets": ["PointA", "PointB"], "notes": "SpeakerNoteHere"},
                {"title": "SlideTwoTitle", "bullets": ["PointC"]}
            ]
        }),
    )
    .await;

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
        assert!(names.iter().any(|n| n == part), "pptx missing {part}: {names:?}");
    }
    let slide1 = zip_entry(&path, "ppt/slides/slide1.xml");
    assert!(slide1.contains("SlideOneTitle"), "slide1 missing title");
    assert!(slide1.contains("PointA"), "slide1 missing bullet");
    let notes1 = zip_entry(&path, "ppt/notesSlides/notesSlide1.xml");
    assert!(notes1.contains("SpeakerNoteHere"), "notes1 missing note text");

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
    assert!(rels1.contains("relationships/notesSlide"), "slide1 links notes");
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

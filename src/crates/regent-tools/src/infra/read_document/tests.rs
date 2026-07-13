use super::*;
use crate::domain::contracts::DenyAll;
use std::io::Write;

/// Builds a minimal-but-valid docx: document.xml + a hyperlink rels part + one
/// embedded image (a PNG signature is enough — nothing decodes it).
fn fake_docx(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("lesson.docx");
    let file = std::fs::File::create(&path).unwrap();
    let mut z = zip::ZipWriter::new(file);
    let opts = zip::write::FileOptions::default();
    z.start_file("word/document.xml", opts).unwrap();
    z.write_all(
        b"<w:document><w:body><w:p><w:r><w:t>Ethics &amp; Society</w:t></w:r></w:p>\
          <w:p><w:r><w:t>Module 1</w:t></w:r></w:p></w:body></w:document>",
    )
    .unwrap();
    z.start_file("word/_rels/document.xml.rels", opts).unwrap();
    z.write_all(
        b"<Relationships>\
          <Relationship Id=\"rId1\" \
           Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" \
           Target=\"https://example.edu/syllabus\" TargetMode=\"External\"/>\
          <Relationship Id=\"rId2\" \
           Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" \
           Target=\"media/image1.png\"/>\
          </Relationships>",
    )
    .unwrap();
    z.start_file("word/media/image1.png", opts).unwrap();
    z.write_all(b"\x89PNG\r\n\x1a\n_fake_image_bytes").unwrap();
    z.finish().unwrap();
    path
}

fn fake_pptx(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("deck.pptx");
    let file = std::fs::File::create(&path).unwrap();
    let mut z = zip::ZipWriter::new(file);
    let opts = zip::write::FileOptions::default();
    for (n, text) in [(1, "Title slide"), (2, "Second point")] {
        z.start_file(format!("ppt/slides/slide{n}.xml"), opts)
            .unwrap();
        z.write_all(format!("<p:sld><a:p><a:r><a:t>{text}</a:t></a:r></a:p></p:sld>").as_bytes())
            .unwrap();
    }
    z.finish().unwrap();
    path
}

async fn dispatch(dir: &Path, target: &std::path::Path) -> String {
    let mut catalog = ToolCatalog::new();
    register_read_document_tool(&mut catalog).unwrap();
    let ctx = ToolContext::new(dir.to_path_buf(), Arc::new(DenyAll))
        .with_scratch_dir(dir.join("scratch"));
    catalog
        .dispatch(
            "read_document",
            &json!({"path": target.to_string_lossy()}).to_string(),
            &ctx,
        )
        .await
}

#[tokio::test]
async fn docx_yields_text_links_and_extracted_images() {
    let dir = tempfile::tempdir().unwrap();
    let out = dispatch(dir.path(), &fake_docx(dir.path())).await;
    let v: Value = serde_json::from_str(&out).unwrap();
    let text = v["text"].as_str().unwrap();
    assert!(text.contains("Ethics & Society"), "{out}");
    assert!(text.contains("Module 1"), "{out}");
    // Hyperlink from the rels part; the internal image rel is NOT a link.
    assert_eq!(v["links"], json!(["https://example.edu/syllabus"]));
    // The embedded image landed in the scratch area, ready for vision_analyze.
    let images = v["images"].as_array().unwrap();
    assert_eq!(images.len(), 1);
    let image_path = std::path::PathBuf::from(images[0].as_str().unwrap());
    assert!(image_path.exists(), "extracted image file exists");
    assert!(
        std::fs::read(&image_path).unwrap().starts_with(b"\x89PNG"),
        "bytes intact"
    );
}

#[tokio::test]
async fn pptx_slides_and_unknown_extension() {
    let dir = tempfile::tempdir().unwrap();
    let out = dispatch(dir.path(), &fake_pptx(dir.path())).await;
    assert!(out.contains("--- Slide 1 ---"), "{out}");
    assert!(out.contains("Title slide"), "{out}");
    assert!(out.contains("Second point"), "{out}");

    let txt = dir.path().join("notes.txt");
    std::fs::write(&txt, "plain").unwrap();
    let out = dispatch(dir.path(), &txt).await;
    assert!(out.contains("error"), "{out}");
    assert!(out.contains("read_file"), "{out}");
}

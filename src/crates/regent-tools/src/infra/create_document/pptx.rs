//! Hand-rolled PPTX: a ZIP of strictly-valid OOXML parts (no python, no
//! templating crate) that PowerPoint and LibreOffice both open. Static parts
//! come from `pptx_scaffold`, content parts from `pptx_xml`.
// ponytail: title + bullets + speaker notes only — one blank layout, one
// theme, plain text boxes (no placeholders, images, tables, or transitions).
// A richer deck wants a real template; this covers "turn an outline into
// slides".

use super::model::DocumentSpec;
use super::{pptx_scaffold as scaf, pptx_xml as xml};
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// Builds the PPTX (a ZIP) bytes for `spec`. Pure in-memory.
pub fn build(spec: &DocumentSpec) -> Result<Vec<u8>, String> {
    let slides = &spec.slides;
    let has_notes = slides.iter().any(|s| s.notes.is_some());

    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut put = |name: &str, data: &str| -> Result<(), String> {
        zip.start_file(name, opts)
            .map_err(|e| format!("pptx zip entry '{name}' failed: {e}"))?;
        zip.write_all(data.as_bytes())
            .map_err(|e| format!("pptx write '{name}' failed: {e}"))
    };

    // Package scaffold.
    put("[Content_Types].xml", &xml::content_types(slides))?;
    put("_rels/.rels", scaf::RELS_ROOT)?;
    put("ppt/presentation.xml", &xml::presentation(slides, has_notes))?;
    put(
        "ppt/_rels/presentation.xml.rels",
        &xml::presentation_rels(slides, has_notes),
    )?;
    put("ppt/theme/theme1.xml", scaf::THEME)?;
    put("ppt/slideMasters/slideMaster1.xml", scaf::SLIDE_MASTER)?;
    put(
        "ppt/slideMasters/_rels/slideMaster1.xml.rels",
        scaf::SLIDE_MASTER_RELS,
    )?;
    put("ppt/slideLayouts/slideLayout1.xml", scaf::SLIDE_LAYOUT)?;
    put(
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
        scaf::SLIDE_LAYOUT_RELS,
    )?;
    if has_notes {
        put("ppt/notesMasters/notesMaster1.xml", scaf::NOTES_MASTER)?;
        put(
            "ppt/notesMasters/_rels/notesMaster1.xml.rels",
            scaf::NOTES_MASTER_RELS,
        )?;
    }

    // One slide (and optional notes slide) per spec entry.
    for (i, slide) in slides.iter().enumerate() {
        let n = i + 1;
        put(&format!("ppt/slides/slide{n}.xml"), &xml::slide(slide))?;
        put(
            &format!("ppt/slides/_rels/slide{n}.xml.rels"),
            &xml::slide_rels(n, slide.notes.is_some()),
        )?;
        if let Some(notes) = &slide.notes {
            put(
                &format!("ppt/notesSlides/notesSlide{n}.xml"),
                &xml::notes_slide(notes),
            )?;
            put(
                &format!("ppt/notesSlides/_rels/notesSlide{n}.xml.rels"),
                &xml::notes_slide_rels(n),
            )?;
        }
    }

    let cursor = zip
        .finish()
        .map_err(|e| format!("pptx finalize failed: {e}"))?;
    Ok(cursor.into_inner())
}

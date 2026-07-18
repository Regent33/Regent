//! Native PPTX packaging: strictly-valid OOXML with no Python runtime.
//! `pptx_xml` supplies the designed layouts, notes, and optional raster art.

use super::model::DocumentSpec;
use super::{pptx_scaffold as scaf, pptx_xml as xml};
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// Builds the PPTX (a ZIP) bytes for `spec`. Pure in-memory.
pub fn build(spec: &DocumentSpec) -> Result<Vec<u8>, String> {
    let slides = &spec.slides;
    let has_notes = slides.iter().any(|slide| slide.notes.is_some());

    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut put = |name: &str, data: &[u8]| -> Result<(), String> {
        zip.start_file(name, opts)
            .map_err(|error| format!("pptx zip entry '{name}' failed: {error}"))?;
        zip.write_all(data)
            .map_err(|error| format!("pptx write '{name}' failed: {error}"))
    };

    put("[Content_Types].xml", xml::content_types(slides).as_bytes())?;
    put("_rels/.rels", scaf::RELS_ROOT.as_bytes())?;
    put(
        "ppt/presentation.xml",
        xml::presentation(slides, has_notes).as_bytes(),
    )?;
    put(
        "ppt/_rels/presentation.xml.rels",
        xml::presentation_rels(slides, has_notes).as_bytes(),
    )?;
    put("ppt/theme/theme1.xml", scaf::THEME.as_bytes())?;
    put(
        "ppt/slideMasters/slideMaster1.xml",
        scaf::SLIDE_MASTER.as_bytes(),
    )?;
    put(
        "ppt/slideMasters/_rels/slideMaster1.xml.rels",
        scaf::SLIDE_MASTER_RELS.as_bytes(),
    )?;
    put(
        "ppt/slideLayouts/slideLayout1.xml",
        scaf::SLIDE_LAYOUT.as_bytes(),
    )?;
    put(
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
        scaf::SLIDE_LAYOUT_RELS.as_bytes(),
    )?;
    if has_notes {
        put(
            "ppt/notesMasters/notesMaster1.xml",
            scaf::NOTES_MASTER.as_bytes(),
        )?;
        put(
            "ppt/notesMasters/_rels/notesMaster1.xml.rels",
            scaf::NOTES_MASTER_RELS.as_bytes(),
        )?;
    }

    for (index, slide) in slides.iter().enumerate() {
        let number = index + 1;
        put(
            &format!("ppt/slides/slide{number}.xml"),
            xml::slide(slide, number, slides.len()).as_bytes(),
        )?;
        put(
            &format!("ppt/slides/_rels/slide{number}.xml.rels"),
            xml::slide_rels(number, slide.notes.is_some(), slide.embedded_image.as_ref())
                .as_bytes(),
        )?;
        if let Some(image) = &slide.embedded_image {
            put(
                &format!("ppt/media/image{number}.{}", image.extension),
                &image.bytes,
            )?;
        }
        if let Some(notes) = &slide.notes {
            put(
                &format!("ppt/notesSlides/notesSlide{number}.xml"),
                xml::notes_slide(notes).as_bytes(),
            )?;
            put(
                &format!("ppt/notesSlides/_rels/notesSlide{number}.xml.rels"),
                xml::notes_slide_rels(number).as_bytes(),
            )?;
        }
    }

    let cursor = zip
        .finish()
        .map_err(|error| format!("pptx finalize failed: {error}"))?;
    Ok(cursor.into_inner())
}

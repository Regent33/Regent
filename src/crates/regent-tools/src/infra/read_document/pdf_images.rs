//! Embedded-image extraction for PDFs — the input half of the OCR rung. A
//! scanned PDF is one big image per page; those live as `/Image` XObject
//! streams. JPEG streams (`DCTDecode`) are written out verbatim; Flate
//! bitmaps are rebuilt for 8-bit RGB/Gray. Exotic encodings (JBIG2, CCITT,
//! JPX) are counted and skipped — extraction never hard-fails the read.
// ponytail: whole-document object scan, not a per-page resource walk — same
// images, quarter of the code; page attribution if OCR output ever needs it.

use std::path::{Path, PathBuf};

/// Enough pages to know what a scan says; the OCR pass is CPU-bound.
const MAX_PDF_IMAGES: usize = 12;
/// Tiny streams are bullets/logos, not page scans.
const MIN_IMAGE_BYTES: usize = 2_048;

/// Extracts up to [`MAX_PDF_IMAGES`] embedded images into `out_dir`, largest
/// first (page scans dwarf decorations). Returns written paths.
pub(super) fn extract_pdf_images(path: &Path, out_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let doc =
        lopdf::Document::load(path).map_err(|e| format!("cannot parse {}: {e}", path.display()))?;
    // Collect candidate image streams first so the biggest win the cap.
    let mut candidates: Vec<(usize, lopdf::ObjectId)> = doc
        .objects
        .iter()
        .filter_map(|(id, object)| {
            let stream = object.as_stream().ok()?;
            let subtype = stream.dict.get(b"Subtype").ok()?.as_name().ok()?;
            (subtype == b"Image" && stream.content.len() >= MIN_IMAGE_BYTES)
                .then_some((stream.content.len(), *id))
        })
        .collect();
    candidates.sort_by_key(|(size, _)| std::cmp::Reverse(*size));

    std::fs::create_dir_all(out_dir)
        .map_err(|e| format!("cannot create {}: {e}", out_dir.display()))?;
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("pdf");
    let mut written = Vec::new();
    let mut skipped = 0usize;
    for (n, (_, id)) in candidates.into_iter().take(MAX_PDF_IMAGES).enumerate() {
        let Ok(object) = doc.get_object(id) else {
            continue;
        };
        let Ok(stream) = object.as_stream() else {
            continue;
        };
        match write_image(stream, out_dir, stem, n) {
            Ok(file) => written.push(file),
            Err(_) => skipped += 1,
        }
    }
    if skipped > 0 {
        tracing::info!(skipped, pdf = %path.display(), "pdf images in unsupported encodings");
    }
    Ok(written)
}

/// One `/Image` stream to a file: JPEG verbatim, Flate bitmap re-encoded PNG.
fn write_image(
    stream: &lopdf::Stream,
    out_dir: &Path,
    stem: &str,
    n: usize,
) -> Result<PathBuf, String> {
    let filters = filter_names(stream);
    if filters.iter().any(|f| f == "DCTDecode") {
        // Content bytes ARE a JPEG (a leading Flate layer is rare; skip those).
        if filters.len() > 1 {
            return Err("layered filters".into());
        }
        let target = out_dir.join(format!("{stem}-img-{n}.jpg"));
        std::fs::write(&target, &stream.content).map_err(|e| e.to_string())?;
        return Ok(target);
    }
    if filters.iter().all(|f| f == "FlateDecode") {
        let data = stream
            .decompressed_content()
            .map_err(|e| format!("flate: {e}"))?;
        let width = dict_i64(stream, b"Width")? as u32;
        let height = dict_i64(stream, b"Height")? as u32;
        let bits = dict_i64(stream, b"BitsPerComponent").unwrap_or(8);
        if bits != 8 {
            return Err(format!("{bits}-bit samples unsupported"));
        }
        let target = out_dir.join(format!("{stem}-img-{n}.png"));
        let pixels = (width as usize) * (height as usize);
        if data.len() >= pixels * 3 {
            image::RgbImage::from_raw(width, height, data[..pixels * 3].to_vec())
                .ok_or("bad rgb buffer")?
                .save(&target)
                .map_err(|e| e.to_string())?;
        } else if data.len() >= pixels {
            image::GrayImage::from_raw(width, height, data[..pixels].to_vec())
                .ok_or("bad gray buffer")?
                .save(&target)
                .map_err(|e| e.to_string())?;
        } else {
            return Err("pixel data shorter than dimensions".into());
        }
        return Ok(target);
    }
    Err(format!("unsupported filters {filters:?}"))
}

fn filter_names(stream: &lopdf::Stream) -> Vec<String> {
    let Ok(filter) = stream.dict.get(b"Filter") else {
        return Vec::new();
    };
    let name = |o: &lopdf::Object| {
        o.as_name()
            .ok()
            .map(|n| String::from_utf8_lossy(n).into_owned())
    };
    match filter {
        lopdf::Object::Array(items) => items.iter().filter_map(name).collect(),
        single => name(single).into_iter().collect(),
    }
}

fn dict_i64(stream: &lopdf::Stream, key: &[u8]) -> Result<i64, String> {
    stream
        .dict
        .get(key)
        .and_then(lopdf::Object::as_i64)
        .map_err(|e| format!("{}: {e}", String::from_utf8_lossy(key)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Dictionary, Object, Stream};

    /// A hand-built PDF whose lone page carries one JPEG XObject — the shape
    /// of every scanned document.
    #[test]
    fn extracts_the_jpeg_page_scan() {
        // Minimal JPEG: SOI + padding past the size floor + EOI.
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0];
        jpeg.extend(std::iter::repeat_n(0u8, MIN_IMAGE_BYTES));
        jpeg.extend([0xFF, 0xD9]);

        let mut doc = lopdf::Document::with_version("1.5");
        let mut dict = Dictionary::new();
        dict.set("Type", Object::Name(b"XObject".to_vec()));
        dict.set("Subtype", Object::Name(b"Image".to_vec()));
        dict.set("Width", Object::Integer(100));
        dict.set("Height", Object::Integer(100));
        dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
        let mut stream = Stream::new(dict, jpeg.clone());
        stream
            .dict
            .set("Length", Object::Integer(jpeg.len() as i64));
        doc.add_object(Object::Stream(stream));

        let dir = tempfile::tempdir().expect("tempdir");
        let pdf_path = dir.path().join("scan.pdf");
        doc.save(&pdf_path).expect("save pdf");

        let out = extract_pdf_images(&pdf_path, &dir.path().join("media")).expect("extract");
        assert_eq!(out.len(), 1);
        let bytes = std::fs::read(&out[0]).expect("read extracted");
        assert_eq!(bytes, jpeg, "JPEG written verbatim");
        assert!(out[0].to_string_lossy().ends_with(".jpg"));
    }

    #[test]
    fn a_text_only_pdf_yields_no_images() {
        let mut doc = lopdf::Document::with_version("1.5");
        doc.add_object(Object::Boolean(true));
        let dir = tempfile::tempdir().expect("tempdir");
        let pdf_path = dir.path().join("text.pdf");
        doc.save(&pdf_path).expect("save pdf");
        let out = extract_pdf_images(&pdf_path, &dir.path().join("media")).expect("extract");
        assert!(out.is_empty());
    }
}

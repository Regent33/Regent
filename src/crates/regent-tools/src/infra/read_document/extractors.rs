//! Local extraction rung: per-format text (plus OOXML links/media via
//! [`super::media`]) assembled into the tool's JSON result. Pure file work —
//! callers run it under `spawn_blocking`.

use super::media;
use serde_json::{Value, json};
use std::path::Path;

/// Per-sheet row cap for spreadsheets — enough to see the data's shape; the
/// catalog's result cap (T6) spills anything bigger anyway.
const MAX_SHEET_ROWS: usize = 500;

/// Routes by extension; assembles the full result (text + links + images).
pub(super) fn extract(path: &Path, ext: &str, media_dir: Option<&Path>) -> Result<Value, String> {
    let (format, text) = match ext {
        "pdf" => pdf_extract::extract_text(path)
            .map(|t| ("pdf", tidy(&t)))
            .map_err(|e| format!("PDF extraction failed for {}: {e}", path.display()))?,
        "docx" => ("docx", docx_text(path)?),
        "pptx" => ("pptx", pptx_text(path)?),
        "xlsx" | "xlsm" | "xls" | "ods" => ("spreadsheet", sheet_text(path)?),
        other => {
            return Err(format!(
                "unsupported extension '.{other}' — read_document handles \
                 pdf/docx/pptx/xlsx/xls/ods; use read_file for text formats"
            ));
        }
    };
    let mut result = json!({"format": format, "text": text});
    if format != "pdf" && ext != "xls" && ext != "ods" {
        let (images, links) = media::media_and_links(path, media_dir);
        if !links.is_empty() {
            result["links"] = json!(links);
        }
        if !images.is_empty() {
            result["images"] = json!(images);
            result["note"] = json!("embedded images extracted — vision_analyze a path to see one");
        }
    }
    if format == "pdf" {
        result["note"] = json!(
            "PDF links are not extracted; a scanned/image-heavy PDF is OCR'd \
             automatically when its text comes back near-empty"
        );
    }
    Ok(result)
}

/// Text of one file inside an OOXML zip.
fn zip_entry(path: &Path, entry: &str) -> Result<String, String> {
    let file =
        std::fs::File::open(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("{} is not a valid archive: {e}", path.display()))?;
    let mut xml = String::new();
    let mut entry = archive
        .by_name(entry)
        .map_err(|e| format!("{}: missing {entry}: {e}", path.display()))?;
    std::io::Read::read_to_string(&mut entry, &mut xml).map_err(|e| e.to_string())?;
    Ok(xml)
}

fn docx_text(path: &Path) -> Result<String, String> {
    let xml = zip_entry(path, "word/document.xml")?;
    Ok(strip_ooxml(&xml, "</w:p>"))
}

fn pptx_text(path: &Path) -> Result<String, String> {
    let file =
        std::fs::File::open(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("{} is not a valid archive: {e}", path.display()))?;
    // Slides are ppt/slides/slideN.xml — collect and sort by N for deck order.
    let mut slides: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_owned()))
        .filter(|n| n.starts_with("ppt/slides/slide") && n.ends_with(".xml"))
        .collect();
    slides.sort_by_key(|name| {
        name.trim_start_matches("ppt/slides/slide")
            .trim_end_matches(".xml")
            .parse::<u32>()
            .unwrap_or(u32::MAX)
    });
    if slides.is_empty() {
        return Err(format!("{}: no slides found", path.display()));
    }
    let mut out = String::new();
    for (i, name) in slides.iter().enumerate() {
        let mut xml = String::new();
        let mut entry = archive.by_name(name).map_err(|e| e.to_string())?;
        std::io::Read::read_to_string(&mut entry, &mut xml).map_err(|e| e.to_string())?;
        out.push_str(&format!("--- Slide {} ---\n", i + 1));
        out.push_str(&strip_ooxml(&xml, "</a:p>"));
        out.push('\n');
    }
    Ok(out)
}

fn sheet_text(path: &Path) -> Result<String, String> {
    use calamine::{Data, Reader};
    let mut workbook = calamine::open_workbook_auto(path)
        .map_err(|e| format!("cannot open workbook {}: {e}", path.display()))?;
    let mut out = String::new();
    for name in workbook.sheet_names().clone() {
        let Ok(range) = workbook.worksheet_range(&name) else {
            continue;
        };
        out.push_str(&format!("--- Sheet: {name} ---\n"));
        let mut clipped = false;
        for (i, row) in range.rows().enumerate() {
            if i >= MAX_SHEET_ROWS {
                clipped = true;
                break;
            }
            let cells: Vec<String> = row
                .iter()
                .map(|c| match c {
                    Data::Empty => String::new(),
                    other => other.to_string(),
                })
                .collect();
            out.push_str(&cells.join("\t"));
            out.push('\n');
        }
        if clipped {
            out.push_str(&format!(
                "[…{MAX_SHEET_ROWS}-row cap reached for this sheet]\n"
            ));
        }
    }
    if out.is_empty() {
        return Err(format!("{}: no readable sheets", path.display()));
    }
    Ok(out)
}

/// OOXML → plain text: paragraph closers become newlines, every tag is
/// stripped, the five XML entities are decoded, blank runs collapse.
fn strip_ooxml(xml: &str, paragraph_close: &str) -> String {
    let with_breaks = xml.replace(paragraph_close, "\n");
    let mut text = String::with_capacity(with_breaks.len() / 4);
    let mut in_tag = false;
    for c in with_breaks.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    let decoded = text
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&");
    tidy(&decoded)
}

/// Collapses runs of blank lines and trims trailing space.
fn tidy(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank_run = 0usize;
    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        out.push_str(trimmed);
        out.push('\n');
    }
    out.trim().to_owned()
}

//! Minimal text PDF via lopdf: A4, Helvetica body / Helvetica-Bold headings,
//! greedy word-wrap and page breaks. Titles and section headings render larger
//! and bold; paragraphs and bullets wrap to the page width.
// ponytail: naive avg-width wrap, real font metrics if layout matters — every
// glyph is treated as 0.5em wide, so proportional text can run a hair short or
// long. Fine for reports; swap in AFM metrics only if a task needs exact fit.

use super::model::DocumentSpec;
use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};

const PAGE_W: f64 = 595.0; // A4 at 72 dpi
const PAGE_H: f64 = 842.0;
const MARGIN: f64 = 72.0;
const BODY_SIZE: f64 = 11.0;
const HEADING_SIZE: f64 = 16.0;
const TITLE_SIZE: f64 = 22.0;

/// A single laid-out line: text, point size, and whether it uses the bold font.
struct Line {
    text: String,
    size: f64,
    bold: bool,
}

/// Builds the PDF bytes for `spec`. Never touches the filesystem — the caller
/// writes the returned buffer.
pub fn build(spec: &DocumentSpec) -> Result<Vec<u8>, String> {
    let lines = layout_lines(spec);
    let pages = paginate(&lines);

    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_regular = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let font_bold = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica-Bold",
        "Encoding" => "WinAnsiEncoding",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_regular, "F2" => font_bold },
    });

    let mut kids: Vec<Object> = Vec::new();
    for page in pages {
        let content = Content { operations: page };
        let content_id = doc.add_object(Stream::new(
            dictionary! {},
            content
                .encode()
                .map_err(|e| format!("PDF content encode failed: {e}"))?,
        ));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
        });
        kids.push(page_id.into());
    }
    let count = kids.len() as i64;
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => count,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), PAGE_W.into(), PAGE_H.into()],
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);

    let mut buf = Vec::new();
    doc.save_to(&mut buf)
        .map_err(|e| format!("PDF save failed: {e}"))?;
    Ok(buf)
}

/// Flattens the spec into wrapped display lines in reading order.
fn layout_lines(spec: &DocumentSpec) -> Vec<Line> {
    let mut lines = Vec::new();
    if let Some(title) = &spec.title {
        wrap_into(&mut lines, title, TITLE_SIZE, true);
        lines.push(Line {
            text: String::new(),
            size: BODY_SIZE,
            bold: false,
        });
    }
    for section in &spec.sections {
        if let Some(heading) = &section.heading {
            wrap_into(&mut lines, heading, HEADING_SIZE, true);
        }
        for para in &section.paragraphs {
            wrap_into(&mut lines, para, BODY_SIZE, false);
            lines.push(Line {
                text: String::new(),
                size: BODY_SIZE,
                bold: false,
            });
        }
        for bullet in &section.bullets {
            wrap_into(&mut lines, &format!("\u{2022} {bullet}"), BODY_SIZE, false);
        }
        lines.push(Line {
            text: String::new(),
            size: BODY_SIZE,
            bold: false,
        });
    }
    lines
}

/// Greedy word-wrap at the usable width using a 0.5em average glyph width.
fn wrap_into(lines: &mut Vec<Line>, text: &str, size: f64, bold: bool) {
    let usable = PAGE_W - 2.0 * MARGIN;
    let max_chars = ((usable / (size * 0.5)).floor() as usize).max(1);
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
        } else if current.chars().count() + 1 + word.chars().count() <= max_chars {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(Line {
                text: std::mem::take(&mut current),
                size,
                bold,
            });
            current.push_str(word);
        }
    }
    lines.push(Line {
        text: current,
        size,
        bold,
    });
}

/// Slices the line stream into pages of PDF text operations.
fn paginate(lines: &[Line]) -> Vec<Vec<Operation>> {
    let mut pages = Vec::new();
    let mut ops = start_page();
    let mut y = PAGE_H - MARGIN;
    for line in lines {
        let leading = line.size * 1.4;
        if y - leading < MARGIN {
            ops.push(Operation::new("ET", vec![]));
            pages.push(std::mem::take(&mut ops));
            ops = start_page();
            y = PAGE_H - MARGIN;
        }
        y -= leading;
        if !line.text.is_empty() {
            let font = if line.bold { "F2" } else { "F1" };
            ops.push(Operation::new("Tf", vec![font.into(), line.size.into()]));
            ops.push(Operation::new(
                "Tm",
                vec![
                    1.into(),
                    0.into(),
                    0.into(),
                    1.into(),
                    MARGIN.into(),
                    y.into(),
                ],
            ));
            ops.push(Operation::new(
                "Tj",
                vec![Object::String(
                    winansi(&line.text),
                    lopdf::StringFormat::Literal,
                )],
            ));
        }
    }
    ops.push(Operation::new("ET", vec![]));
    pages.push(ops);
    pages
}

fn start_page() -> Vec<Operation> {
    vec![Operation::new("BT", vec![])]
}

/// Encodes text as WinAnsi (CP1252) bytes — the encoding declared on both
/// fonts. Raw UTF-8 in a Type1 text stream renders as mojibake (the bullet
/// `•` became `â€¢`), so every character is mapped; unmappable ones become
/// `?` rather than silently corrupting the neighbours.
fn winansi(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| match c {
            '€' => 0x80,
            '‚' => 0x82,
            'ƒ' => 0x83,
            '„' => 0x84,
            '…' => 0x85,
            '†' => 0x86,
            '‡' => 0x87,
            'ˆ' => 0x88,
            '‰' => 0x89,
            'Š' => 0x8A,
            '‹' => 0x8B,
            'Œ' => 0x8C,
            'Ž' => 0x8E,
            '\u{2018}' => 0x91,
            '\u{2019}' => 0x92,
            '\u{201C}' => 0x93,
            '\u{201D}' => 0x94,
            '\u{2022}' => 0x95,
            '\u{2013}' => 0x96,
            '\u{2014}' => 0x97,
            '˜' => 0x98,
            '™' => 0x99,
            'š' => 0x9A,
            '›' => 0x9B,
            'œ' => 0x9C,
            'ž' => 0x9E,
            'Ÿ' => 0x9F,
            c if (c as u32) < 0x80 || (0xA0..=0xFF).contains(&(c as u32)) => c as u32 as u8,
            _ => b'?',
        })
        .collect()
}

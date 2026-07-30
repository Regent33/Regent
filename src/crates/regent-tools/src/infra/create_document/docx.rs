//! Word document via docx-rs: the title and section headings become bold,
//! larger runs; paragraphs are plain; bullets hang off a single bullet-format
//! numbering definition.
//!
//! Word used to be the one format that ignored the `Theme` entirely — every
//! document Regent wrote came out as the same black Calibri, no matter the
//! subject. It now takes the same resolved palette and font pairing as the PDF
//! and the deck, so a set of documents on one topic reads as one designed
//! system.
// ponytail: headings are themed bold+sized runs, not Word "Heading 1" styles —
// no outline/navigation pane. Add real styles only if a task needs the TOC.

use super::model::{DocumentSpec, Table as SpecTable};
use super::theme::Theme;
use docx_rs::{
    AbstractNumbering, Docx, IndentLevel, Level, LevelJc, LevelText, Numbering, NumberingId,
    Paragraph, Pic, Run, RunFonts, SpecialIndentType, Start, Table, TableCell, TableRow,
};
use std::io::Cursor;

const BULLET_NUM_ID: usize = 1;
const TITLE_HALF_PT: usize = 44; // 22pt
const HEADING_HALF_PT: usize = 32; // 16pt

/// A run in the theme's body face and text color — the default for prose.
fn body_run(theme: &Theme, text: &str) -> Run {
    Run::new().add_text(text).color(&theme.text).fonts(
        RunFonts::new()
            .ascii(&theme.body_font)
            .hi_ansi(&theme.body_font),
    )
}

/// A run in the theme's display face, for the title and section headings.
fn title_run(theme: &Theme, text: &str, size: usize, color: &str) -> Run {
    Run::new()
        .add_text(text)
        .bold()
        .size(size)
        .color(color)
        .fonts(
            RunFonts::new()
                .ascii(&theme.title_font)
                .hi_ansi(&theme.title_font),
        )
}

/// Letter minus one-inch margins, in EMU (914,400 per inch) — the width of the
/// text column an image should never exceed.
const TEXT_COLUMN_EMU: u32 = 6 * 914_400;

/// Scale an image to `max_width` EMU, keeping its aspect ratio. Dimensions come
/// from the hydrator, which already decoded the image; a zero width (nothing
/// sane produces one) falls back to a 4:3 box rather than dividing by it.
fn fit_width(width: u32, height: u32, max_width: u32) -> (u32, u32) {
    if width == 0 {
        return (max_width, max_width * 3 / 4);
    }
    let scaled = u64::from(max_width) * u64::from(height) / u64::from(width);
    (max_width, u32::try_from(scaled).unwrap_or(max_width))
}

/// A real Word table (`w:tbl`), not a tab-aligned imitation — it stays
/// editable, sortable and selectable in Word, which is the entire reason to ask
/// for a .docx rather than a PDF.
///
/// The header row is the theme's display face on the accent colour so it reads
/// as a header without depending on Word's table styles, which travel badly
/// between Word versions and LibreOffice.
fn spec_table(theme: &Theme, table: &SpecTable) -> Table {
    let cell = |text: &str, header: bool| {
        let run = if header {
            title_run(theme, text, 20, &theme.accent)
        } else {
            body_run(theme, text)
        };
        TableCell::new().add_paragraph(Paragraph::new().add_run(run))
    };
    let width = table.columns();
    let mut rows: Vec<TableRow> = Vec::new();
    if !table.headers.is_empty() {
        let mut headers = table.headers.clone();
        headers.resize(width, String::new());
        rows.push(TableRow::new(
            headers.iter().map(|h| cell(h, true)).collect(),
        ));
    }
    for row in table.padded_rows() {
        rows.push(TableRow::new(row.iter().map(|c| cell(c, false)).collect()));
    }
    Table::new(rows)
}

/// Builds the DOCX bytes for `spec`. Pure in-memory; the caller writes them.
pub fn build(spec: &DocumentSpec, theme: &Theme) -> Result<Vec<u8>, String> {
    let mut doc = Docx::new();

    if let Some(title) = &spec.title {
        // The cover color, so the title carries the document's identity the way
        // the PDF cover panel does.
        doc = doc.add_paragraph(Paragraph::new().add_run(title_run(
            theme,
            title,
            TITLE_HALF_PT,
            &theme.cover_background,
        )));
    }

    let mut used_bullets = false;
    for section in &spec.sections {
        if let Some(heading) = &section.heading {
            doc = doc.add_paragraph(Paragraph::new().add_run(title_run(
                theme,
                heading,
                HEADING_HALF_PT,
                &theme.accent,
            )));
        }
        for para in &section.paragraphs {
            doc = doc.add_paragraph(Paragraph::new().add_run(body_run(theme, para)));
        }
        for bullet in &section.bullets {
            used_bullets = true;
            doc = doc.add_paragraph(
                Paragraph::new()
                    .add_run(body_run(theme, bullet))
                    .numbering(NumberingId::new(BULLET_NUM_ID), IndentLevel::new(0)),
            );
        }
        // Word could not embed a picture at all, so a section that asked for a
        // figure quietly lost it. Sized to the text column (6in) with the
        // aspect ratio kept, because a full-resolution photo dropped in at its
        // native pixel size runs off the page.
        if let Some(image) = &section.image_render {
            let (width, height) = fit_width(image.width, image.height, TEXT_COLUMN_EMU);
            doc = doc.add_paragraph(
                Paragraph::new()
                    .add_run(Run::new().add_image(Pic::new(&image.bytes).size(width, height))),
            );
            if !image.alt.is_empty() {
                doc = doc.add_paragraph(
                    Paragraph::new().add_run(title_run(theme, &image.alt, 18, &theme.muted)),
                );
            }
        }
        // After the prose: a table is the evidence for what was just said.
        if let Some(table) = &section.table {
            doc = doc.add_table(spec_table(theme, table));
            if let Some(caption) = &table.caption {
                doc = doc.add_paragraph(
                    Paragraph::new().add_run(title_run(theme, caption, 18, &theme.muted)),
                );
            }
        }
    }

    if used_bullets {
        doc = doc
            .add_abstract_numbering(
                AbstractNumbering::new(BULLET_NUM_ID).add_level(
                    Level::new(
                        0,
                        Start::new(1),
                        // "bullet" format + a literal bullet glyph = an unordered list.
                        docx_rs::NumberFormat::new("bullet"),
                        LevelText::new("\u{2022}"),
                        LevelJc::new("left"),
                    )
                    .indent(
                        Some(720),
                        Some(SpecialIndentType::Hanging(360)),
                        None,
                        None,
                    ),
                ),
            )
            .add_numbering(Numbering::new(BULLET_NUM_ID, BULLET_NUM_ID));
    }

    let mut cursor = Cursor::new(Vec::new());
    doc.build()
        .pack(&mut cursor)
        .map_err(|e| format!("DOCX pack failed: {e}"))?;
    Ok(cursor.into_inner())
}

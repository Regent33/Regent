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

use super::model::DocumentSpec;
use super::theme::Theme;
use docx_rs::{
    AbstractNumbering, Docx, IndentLevel, Level, LevelJc, LevelText, Numbering, NumberingId,
    Paragraph, Run, RunFonts, SpecialIndentType, Start,
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

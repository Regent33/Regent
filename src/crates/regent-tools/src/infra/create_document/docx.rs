//! Word document via docx-rs: the title and section headings become bold,
//! larger runs; paragraphs are plain; bullets hang off a single bullet-format
//! numbering definition.
// ponytail: headings are just bold+sized runs, not Word "Heading 1" styles —
// no outline/navigation pane. Add real styles only if a task needs the TOC.

use super::model::DocumentSpec;
use docx_rs::{
    AbstractNumbering, Docx, IndentLevel, Level, LevelJc, LevelText, Numbering, NumberingId,
    Paragraph, Run, SpecialIndentType, Start,
};
use std::io::Cursor;

const BULLET_NUM_ID: usize = 1;
const TITLE_HALF_PT: usize = 44; // 22pt
const HEADING_HALF_PT: usize = 32; // 16pt

/// Builds the DOCX bytes for `spec`. Pure in-memory; the caller writes them.
pub fn build(spec: &DocumentSpec) -> Result<Vec<u8>, String> {
    let mut doc = Docx::new();

    if let Some(title) = &spec.title {
        doc = doc.add_paragraph(
            Paragraph::new().add_run(Run::new().add_text(title).bold().size(TITLE_HALF_PT)),
        );
    }

    let mut used_bullets = false;
    for section in &spec.sections {
        if let Some(heading) = &section.heading {
            doc = doc.add_paragraph(
                Paragraph::new()
                    .add_run(Run::new().add_text(heading).bold().size(HEADING_HALF_PT)),
            );
        }
        for para in &section.paragraphs {
            doc = doc.add_paragraph(Paragraph::new().add_run(Run::new().add_text(para)));
        }
        for bullet in &section.bullets {
            used_bullets = true;
            doc = doc.add_paragraph(
                Paragraph::new()
                    .add_run(Run::new().add_text(bullet))
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
                    .indent(Some(720), Some(SpecialIndentType::Hanging(360)), None, None),
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

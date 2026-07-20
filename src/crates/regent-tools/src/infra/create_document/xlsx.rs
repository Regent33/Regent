//! Spreadsheet via rust_xlsxwriter: one worksheet per `sheet` spec. Numeric
//! cells are written as real numbers (right-aligned, usable in formulas);
//! everything else is written as a string.
//!
//! Excel used to ignore the `Theme` entirely — every workbook came out with the
//! same plain bold header. The header row now carries the document's accent
//! color and font, is frozen so it stays visible while scrolling, and columns
//! are widened to their content, which is the difference between a generated
//! grid and something a person would hand over.
// ponytail: widths are measured on character count, not rendered text width —
// close enough for a hand-off, and it needs no font metrics.

use super::model::{DocumentSpec, Sheet};
use super::theme::Theme;
use rust_xlsxwriter::{Color, Format, Workbook};
use serde_json::Value;

/// Column width bounds, in characters: narrow enough that a one-word column
/// doesn't look empty, capped so one long cell can't push the rest off-screen.
const MIN_COL_WIDTH: f64 = 8.0;
const MAX_COL_WIDTH: f64 = 60.0;

/// Builds the XLSX bytes for `spec`. Returns the workbook buffer for the caller
/// to write.
pub fn build(spec: &DocumentSpec, theme: &Theme) -> Result<Vec<u8>, String> {
    let mut workbook = Workbook::new();
    let header = Format::new()
        .set_bold()
        .set_font_name(&theme.body_font)
        .set_font_color(hex_color(&theme.cover_text))
        .set_background_color(hex_color(&theme.accent));
    let body = Format::new()
        .set_font_name(&theme.body_font)
        .set_font_color(hex_color(&theme.text));

    for sheet in &spec.sheets {
        let ws = workbook.add_worksheet();
        ws.set_name(&sheet.name)
            .map_err(|e| format!("invalid sheet name '{}': {e}", sheet.name))?;
        write_grid(ws, sheet, &header, &body)?;
        fit_columns(ws, sheet)?;
        if sheet.header {
            ws.set_freeze_panes(1, 0)
                .map_err(|e| format!("freeze panes failed on '{}': {e}", sheet.name))?;
        }
    }

    workbook
        .save_to_buffer()
        .map_err(|e| format!("XLSX save failed: {e}"))
}

/// Parse a 6-hex theme color (no '#') into an xlsxwriter color. A malformed
/// value falls back to black rather than failing the whole workbook — a theme is
/// decoration, and losing the spreadsheet over a bad hex digit is the wrong
/// trade.
fn hex_color(value: &str) -> Color {
    u32::from_str_radix(value.trim_start_matches('#'), 16).map_or(Color::Black, Color::RGB)
}

/// Widen each column to its widest cell, clamped. Without this every column is
/// Excel's default 8.43 and long text is visibly truncated.
fn fit_columns(ws: &mut rust_xlsxwriter::Worksheet, sheet: &Sheet) -> Result<(), String> {
    let columns = sheet.rows.iter().map(Vec::len).max().unwrap_or(0);
    for col in 0..columns {
        let widest = sheet
            .rows
            .iter()
            .filter_map(|row| row.get(col))
            .map(|cell| cell_text(cell).chars().count())
            .max()
            .unwrap_or(0);
        // +2 for the cell padding Excel does not include in the width unit.
        let width = ((widest + 2) as f64).clamp(MIN_COL_WIDTH, MAX_COL_WIDTH);
        ws.set_column_width(col as u16, width)
            .map_err(|e| format!("column width failed on '{}' c{col}: {e}", sheet.name))?;
    }
    Ok(())
}

fn cell_text(cell: &Value) -> String {
    match cell {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn write_grid(
    ws: &mut rust_xlsxwriter::Worksheet,
    sheet: &Sheet,
    bold: &Format,
    body: &Format,
) -> Result<(), String> {
    for (r, row) in sheet.rows.iter().enumerate() {
        let header_row = sheet.header && r == 0;
        for (c, cell) in row.iter().enumerate() {
            let (row_i, col_i) = (r as u32, c as u16);
            let outcome = match cell {
                Value::Number(n) if n.as_f64().is_some() => {
                    let v = n.as_f64().unwrap();
                    let format = if header_row { bold } else { body };
                    ws.write_number_with_format(row_i, col_i, v, format)
                }
                Value::Null => ws.write_string(row_i, col_i, ""),
                other => {
                    let text = cell_text(other);
                    let format = if header_row { bold } else { body };
                    ws.write_string_with_format(row_i, col_i, &text, format)
                }
            };
            outcome.map_err(|e| {
                format!("cell write failed at sheet '{}' r{r}c{c}: {e}", sheet.name)
            })?;
        }
    }
    Ok(())
}

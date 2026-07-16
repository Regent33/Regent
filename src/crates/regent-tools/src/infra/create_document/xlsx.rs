//! Spreadsheet via rust_xlsxwriter: one worksheet per `sheet` spec. Numeric
//! cells are written as real numbers (right-aligned, usable in formulas);
//! everything else is written as a string. An optional bold header row.
// ponytail: no column widths, types, or number formats beyond string-vs-number
// — the grid the model hands us, verbatim. Add formatting only on demand.

use super::model::{DocumentSpec, Sheet};
use rust_xlsxwriter::{Format, Workbook};
use serde_json::Value;

/// Builds the XLSX bytes for `spec`. Returns the workbook buffer for the caller
/// to write.
pub fn build(spec: &DocumentSpec) -> Result<Vec<u8>, String> {
    let mut workbook = Workbook::new();
    let bold = Format::new().set_bold();

    for sheet in &spec.sheets {
        let ws = workbook.add_worksheet();
        ws.set_name(&sheet.name)
            .map_err(|e| format!("invalid sheet name '{}': {e}", sheet.name))?;
        write_grid(ws, sheet, &bold)?;
    }

    workbook
        .save_to_buffer()
        .map_err(|e| format!("XLSX save failed: {e}"))
}

fn write_grid(
    ws: &mut rust_xlsxwriter::Worksheet,
    sheet: &Sheet,
    bold: &Format,
) -> Result<(), String> {
    for (r, row) in sheet.rows.iter().enumerate() {
        let header_row = sheet.header && r == 0;
        for (c, cell) in row.iter().enumerate() {
            let (row_i, col_i) = (r as u32, c as u16);
            let outcome = match cell {
                Value::Number(n) if n.as_f64().is_some() => {
                    let v = n.as_f64().unwrap();
                    if header_row {
                        ws.write_number_with_format(row_i, col_i, v, bold)
                    } else {
                        ws.write_number(row_i, col_i, v)
                    }
                }
                Value::Null => ws.write_string(row_i, col_i, ""),
                other => {
                    let text = match other {
                        Value::String(s) => s.clone(),
                        v => v.to_string(),
                    };
                    if header_row {
                        ws.write_string_with_format(row_i, col_i, &text, bold)
                    } else {
                        ws.write_string(row_i, col_i, &text)
                    }
                }
            };
            outcome.map_err(|e| {
                format!("cell write failed at sheet '{}' r{r}c{c}: {e}", sheet.name)
            })?;
        }
    }
    Ok(())
}

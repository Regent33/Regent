---
name: doc-forge
description: Build designed pptx, docx, xlsx, PDF, and CSV files.
---

# Doc Forge — designed documents, not plain dumps

Use for ANY request for a presentation, deck, report, spreadsheet, invoice,
one-pager, or data export.

You BUILD real files with real design. Never answer a "make me a deck/report/
spreadsheet" request with markdown in chat — produce the file, tell the user
its path, and describe the design choices you made.

## Step 0 — pick the runtime (once per session)

Run in the terminal and keep the first that works:
1. `python --version` → use the Python lane (python-pptx, python-docx, openpyxl, reportlab)
2. `bun --version` or `node --version` → use the JS lane (pptxgenjs, docx, exceljs, pdf-lib)
3. Neither → CSV/HTML lanes still work with plain file writes; for pptx/docx/xlsx
   tell the user which runtime to install (one line), don't fake it.

Install into a scratch dir, never globally:
- Python: `pip install python-pptx python-docx openpyxl reportlab --quiet`
- JS: `bun add pptxgenjs docx exceljs pdf-lib` (in a temp workdir) or `npx -y`

## Design system (applies to EVERY format)

Decide these BEFORE writing code, and state them to the user:
- **Palette**: one dominant brand color + one accent + neutrals. Dark text on
  light (#1a1a2e on #fafafa) or the inverse for title slides. Never default-blue-on-white.
- **Type scale**: two fonts max (display + body). Sizes step ×1.25 (12/15/19/24/30/38).
- **Layout**: generous margins (≥8% of page), strict alignment grid, one idea
  per slide/section. White space is a feature.
- **Data**: numbers get charts (native chart APIs below) or styled tables with
  zebra rows, right-aligned numerals, bold headers on the brand color.
- Vary layouts across slides/pages (title / two-column / big-number / quote /
  chart) — identical slides read as lazy.

## PPTX (PowerPoint)

Python lane — `python-pptx`:
- 16:9: `prs.slide_width = Inches(13.333); prs.slide_height = Inches(7.5)`
- Full-bleed color blocks: `slide.shapes.add_shape(MSO_SHAPE.RECTANGLE, 0, 0, w, h)` with
  `fill.solid(); fill.fore_color.rgb = RGBColor(...)`, then text boxes on top.
- Charts: `slide.shapes.add_chart(XL_CHART_TYPE.COLUMN_CLUSTERED, x, y, cx, cy, chart_data)`.
- Build: title slide (accent block + big display type) → agenda → content
  (alternate layouts) → closing. 8–14 slides for a standard deck.

JS lane — `pptxgenjs`: `pptx.defineSlideMaster` for the design system, then
`addSlide({masterName})`; `addChart(pptx.ChartType.bar, data, opts)`.

## DOCX (Word)

Python lane — `python-docx`: set the Normal style font + size once; real
heading hierarchy (Heading 1/2/3 with brand-colored `run.font.color.rgb`);
tables with `table.style = 'Light Grid Accent 1'` then override header row
bold + shading via `_tc.get_or_add_tcPr()` shading XML. Cover page: large
display title + subtitle + date, page break, auto table of contents field.

JS lane — `docx` npm: `new Document({styles: {...}})`, `HeadingLevel`, `Table`.

## XLSX (Excel)

Python lane — `openpyxl`: freeze header row (`ws.freeze_panes = "A2"`), header
fill `PatternFill(start_color=BRAND)` + white bold font, `ws.auto_filter`,
column widths from content, number formats (`#,##0.00`, `0.0%`), conditional
formatting (`ColorScaleRule`) on KPI columns, and a native chart
(`BarChart`/`LineChart` + `Reference`) on a summary sheet.
JS lane — `exceljs` (same features, `worksheet.addConditionalFormatting`).

## PDF

Best design for the least code: write styled HTML+CSS (you are excellent at
this — full design system, flex/grid layouts, print CSS `@page` margins), then:
1. Headless Chrome/Edge if present:
   `msedge --headless --disable-gpu --print-to-pdf="out.pdf" file:///abs/in.html`
   (try `chrome` too). This is the "stunning" path — use it when available.
2. Else Python `reportlab` (deliberate layout: `canvas` + `Platypus` flowables).
3. Else tell the user which of the two to enable. Never emit a .txt renamed to .pdf.

## CSV

Plain file write — RFC 4180: quote fields containing `",\n`, UTF-8 **with BOM**
(`﻿`) so Excel opens it correctly, CRLF line endings, header row always.
No design lane needed — correctness IS the design.

## Delivery

- Default output dir: the workspace (or `~/Documents` when the user gives no
  location). Absolute path in the reply.
- On a platform session (Telegram/Discord/…), send the file with `send_file`
  (load it via load_tools if needed).
- Verify before claiming success: the file exists and is non-trivial
  (`ls`), and for zips (pptx/docx/xlsx) it opens — e.g. `python -c
  "from pptx import Presentation; Presentation('out.pptx')"`.

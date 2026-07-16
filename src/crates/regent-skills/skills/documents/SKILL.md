---
name: documents
description: Read and create PDF, Word, PowerPoint, Excel files.
version: 1.0.0
created_by: bundled
pinned: true
tags: [documents, office, pdf]
---

Working with office documents and PDFs. Follow these paths — do not improvise
terminal one-liners first.

## Reading
Use the `read_document` tool for `.pdf`, `.docx`, `.pptx`, `.xlsx`/`.xls`/`.ods`
(load it via `load_tools` if it isn't in your catalog). It climbs a ladder for
you: a capable provider reads the PDF directly (pictures and layout included);
otherwise text, hyperlinks, and embedded images are extracted in-process; and
when the text comes back near-empty (scanned pages, photo decks) it OCRs the
images locally with PP-OCR — no Python, no external installs. The result's
`source`/`note`/`ocr` fields say which rung produced it. `read_file` stays the
tool for plain-text formats; `vision_analyze` an extracted image path when you
need to SEE a figure rather than read its text.

## Creating
Use the `create_document` tool (load it via `load_tools` if it isn't in your
catalog) — it builds real PDF/DOCX/PPTX/XLSX files from a structured spec, no
HTML round-trip, no Python:
- **PDF/Word** — pass `sections` (heading, paragraphs, bullets each).
- **PowerPoint** — pass `slides` (title, bullets, speaker notes per slide).
- **Excel** — pass `sheets` (name + rows of strings/numbers, `header: true`
  bolds the first row; numbers stay real numbers). For a quick data hand-off,
  CSV via `write_file` is still the simplest thing Excel opens natively.

### Fallback: HTML-print-to-PDF
Reach for this only when a PDF needs layout `create_document` can't express —
custom CSS, precise pixel layout, print-only decoration. Write a
self-contained HTML file, then print it headlessly with the browser every
machine already has:
- Windows: `"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe" --headless --disable-gpu --print-to-pdf="out.pdf" "file:///C:/path/in.html"`
- macOS/Linux: `chrome --headless --disable-gpu --print-to-pdf=out.pdf in.html`
Use CSS `@page` for margins/size and `page-break-after` between sections.

## Platform traps
- **Windows: run `python`, never `python3`** — `python3` is often a Store/shim
  executable that hangs for minutes and exits 0 with NO output. Verify with
  `python --version` before relying on any Python path.
- A tool that returns exit 0 with empty stdout after a long wait did NOT work —
  treat it as a failure and switch approach, don't retry it.
- Always quote paths (spaces are common in user files), and copy user uploads
  to your artifacts directory before processing.

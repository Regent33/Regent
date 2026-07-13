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
(load it via `load_tools` if it isn't in your catalog). It extracts plain text
in-process — no Python, no external installs. `read_file` stays the tool for
plain-text formats.

## Creating
Build content as **HTML first**, then convert:

- **PDF** — write a self-contained HTML file, then print it headlessly with the
  browser every machine already has:
  - Windows: `"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe" --headless --disable-gpu --print-to-pdf="out.pdf" "file:///C:/path/in.html"`
  - macOS/Linux: `chrome --headless --disable-gpu --print-to-pdf=out.pdf in.html`
  Use CSS `@page` for margins/size and `page-break-after` between sections.
- **Word (.docx)** — a docx is a ZIP of XML. Generate `word/document.xml`
  (paragraphs = `<w:p><w:r><w:t>text</w:t></w:r></w:p>`) plus the standard
  `[Content_Types].xml` and `_rels/.rels`, then zip them. Where Python is
  verified working, `python-docx` is simpler.
- **PowerPoint (.pptx)** — same ZIP-of-XML idea (`ppt/slides/slideN.xml`), but
  hand-rolling a valid deck is fiddly: prefer `python-pptx` when Python works;
  otherwise deliver the deck as one HTML file per slide + the PDF print of it,
  and say so.
- **Excel (.xlsx)** — for data hand-off, CSV is usually enough (Excel opens it
  natively) — ask before reaching for a real .xlsx writer.

## Platform traps
- **Windows: run `python`, never `python3`** — `python3` is often a Store/shim
  executable that hangs for minutes and exits 0 with NO output. Verify with
  `python --version` before relying on any Python path.
- A tool that returns exit 0 with empty stdout after a long wait did NOT work —
  treat it as a failure and switch approach, don't retry it.
- Always quote paths (spaces are common in user files), and copy user uploads
  to your artifacts directory before processing.

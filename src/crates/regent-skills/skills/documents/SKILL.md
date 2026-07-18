---
name: documents
description: Read and create PDF, Word, PowerPoint, Excel files.
version: 1.1.0
created_by: bundled
pinned: true
tags: [documents, office, pdf]
---

Working with office documents and PDFs. Follow these paths; do not improvise
terminal one-liners first.

## Reading
Use the `read_document` tool for `.pdf`, `.docx`, `.pptx`, `.xlsx`/`.xls`/`.ods`
(load it via `load_tools` if it is not in your catalog). It climbs a ladder for
you: a capable provider reads the PDF directly (pictures and layout included);
otherwise text, hyperlinks, and embedded images are extracted in-process; and
when the text comes back near-empty (scanned pages, photo decks) it OCRs the
images locally with PP-OCR, no Python or external installs. The result's
`source`/`note`/`ocr` fields say which rung produced it. `read_file` stays the
tool for plain-text formats; use `vision_analyze` on an extracted image path
when you need to SEE a figure rather than read its text.

## Creating
Use the `create_document` tool (load it via `load_tools` if it is not in your
catalog). It builds real PDF/DOCX/PPTX/XLSX files from a structured spec with
no HTML round-trip and no Python:

- **PDF/Word:** pass `sections` (heading, paragraphs, bullets each).
- **PowerPoint:** first form a short narrative: audience/job, one throughline,
  then one claim per slide. Keep the cover minimal and body slides concise
  (usually 2-4 bullets, not pasted source paragraphs). Save every deck in its
  own named folder, for example
  `stanford-ai-roadmap-presentation/stanford-ai-roadmap.pptx`; never choose a
  bare artifacts-root filename yourself. Pass `slides` with `title`, optional
  `subtitle`, concise `bullets`, optional `notes`, and optional
  `image: {path, alt_text}`. The native writer supplies the visual system and
  split-image layouts.
  - When the user asks for pictures or design, load the `media` toolset and use
    `image_generation` for 2-4 purposeful, slide-specific visuals, or reuse
    relevant images extracted by `read_document`. Use a different image per
    concept; never repeat one decorative image throughout the deck. If image
    generation is unavailable, continue with the designed native layouts and
    say so. Do not install packages or fall back to a plain deck.
  - Finish by reporting the exact `created` file and `folder` returned by the
    tool. Do not claim completion before `create_document` succeeds.
- **Excel:** pass `sheets` (name plus rows of strings/numbers; `header: true`
  bolds the first row and numbers stay numeric). For a quick data hand-off, CSV
  via `write_file` is still the simplest thing Excel opens natively.

### Fallback: HTML-print-to-PDF
Reach for this only when a PDF needs layout `create_document` cannot express:
custom CSS, precise pixel layout, or print-only decoration. Write a
self-contained HTML file, then print it headlessly with the browser already on
the machine:

- Windows: `"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe" --headless --disable-gpu --print-to-pdf="out.pdf" "file:///C:/path/in.html"`
- macOS/Linux: `chrome --headless --disable-gpu --print-to-pdf=out.pdf in.html`

Use CSS `@page` for margins/size and `page-break-after` between sections.

## Platform traps

- **Windows: run `python`, never `python3`:** `python3` is often a Store/shim
  executable that hangs for minutes and exits 0 with no output. Verify with
  `python --version` before relying on any Python path.
- A tool that returns exit 0 with empty stdout after a long wait did not work.
  Treat it as a failure and switch approach; do not retry it.
- Always quote paths (spaces are common in user files), and copy user uploads
  to the artifacts directory before processing.

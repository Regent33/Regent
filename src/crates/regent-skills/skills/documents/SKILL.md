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
catalog). It builds real PDF/DOCX/PPTX/XLSX from a structured spec — no Python.
PDF and PowerPoint render through a designed pipeline (themed HTML→PDF via the
installed browser; native editable PptxGenJS decks) when it is available, falling
back to built-in writers otherwise; Word and Excel are always native.

**Make it look designed, not generic.** A default-everything document comes out
static — actively drive the three variety knobs:
- **Theme that fits the subject.** Pass a `theme`: a preset name (`midnight`,
  `warm-editorial`, `mono`, `forest`, `royal`) or — better for anything meant to
  look bespoke — a custom palette object (`background`, `text`, `accent`, `muted`,
  `coverBackground`, `coverText`, `titleFont`, `bodyFont`; optional `base` to
  inherit from). Design it for the topic: deep navy + a bright accent for tech, a
  warm editorial palette for humanities, etc. Omit it and a theme is derived from
  the content (so two docs differ), but a chosen one always beats the default.
- **Vary layouts — don't make every slide bullets.** A deck of 15 identical
  bullet slides is the #1 cause of "static." Open on `cover`; drop a `section`
  divider before each major part; use `split` when a slide has a photo; `grid` to
  show an agenda or a set of enumerated points as numbered cards; `content` for
  true bullet slides; `chart` for data. Set each slide's `layout`, or let it be
  inferred from the slide's shape.
- **Add photos (keyless, no setup).** The easiest way to illustrate a slide is
  `image: {query: "..."}` — a relevant, commercially-licensed photo is fetched and
  embedded automatically, no API key. Put one purposeful, slide-specific photo on
  3–5 slides (cover, section dividers, key concepts); a different query each time,
  never one decorative image repeated. You can also pass `url` (a direct link) or
  `path` (a local PNG/JPEG from `image_generation` or `read_document`). Any source
  that can't be resolved comes back in `image_notes` — refine the query and edit.

**Check your work with your eyes — the vision QA loop.**
1. Create with `preview: true`. The result carries a `preview` PNG path, rendered
   in the background and fully headless (no window pops up while the user is on
   the machine). PDFs always preview; decks need LibreOffice installed (a
   `preview_note` tells you when it's missing — the deck itself is still created).
2. `vision_analyze` that preview and ask pointedly: is any text overflowing or
   clipped, is contrast readable, are slides overcrowded or empty, any typos?
3. If it flags problems, fix with `operation: "edit"` + a `patch` (only the
   fields to change; arrays replace wholesale), then preview again. Repeat until
   it looks right.

**Editing a document you made:** `operation: "edit"` with the same `path` and a
`patch` reloads its saved spec and re-renders — no need to restate the whole
thing. Third-party files have no saved spec: `read_document` them and create anew.

Format specifics:

- **PDF/Word:** pass `sections` (heading, paragraphs, bullets each). A PDF
  section may also carry `image: {query|url|path, alt_text}` — a figure is fetched
  keylessly (same as slide images) and embedded in the report, so reports aren't
  wall-to-wall text.
- **PowerPoint:** first form a short narrative: audience/job, one throughline,
  then one claim per slide. Keep the cover minimal and body slides concise
  (usually 2-4 bullets, not pasted source paragraphs). Save every deck in its
  own named folder, for example
  `stanford-ai-roadmap-presentation/stanford-ai-roadmap.pptx`; never choose a
  bare artifacts-root filename yourself. Pass `slides` with `title`, optional
  `subtitle`, concise `bullets`, optional `notes`, and optional
  `image: {path, alt_text}`. The native writer supplies the visual system and
  split-image layouts.
  - Illustrate slides with `image: {query: "..."}` — keyless stock photos, the
    simplest path (see "Add photos" above). For original/generated art instead of
    a photo, load the `media` toolset, call `image_generation`, and pass the file
    it returns as `image: {path}`. Never install packages or fall back to a plain
    deck for images.
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

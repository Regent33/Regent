# ADR-040 — Document rendering pipeline (themed HTML/PptxGenJS via a CLI sidecar)

Status: accepted · 2026-07-19 · extends ADR-037

**Context:** ADR-037's native writers each emit ONE fixed visual recipe, so every
PDF and deck looked identical; there was also no way to edit a generated file or
to let the model see and fix its own output. Frontier practice (verified against
Anthropic's own document skills) is PptxGenJS for decks + HTML→Chromium for PDF,
plus a create→render→vision→fix loop. The quality gap is the design layer, not the
file library.

**Decision:** `create_document` renders PDF as themed Tera HTML → headless
Chromium and PPTX via PptxGenJS, driven through a hidden `regent __render` sidecar
(one JSON job on stdin, one JSON result with base64 bytes on stdout) so no browser
or Node runtime is bundled into the deacon. DOCX/XLSX stay native. Themes are open
— a preset name, a full custom palette, or a default seeded from the content so
documents differ. Generated files carry a `<file>.regent.json` manifest enabling
`operation:"edit"` (RFC 7386 merge-patch → re-render). `preview:true` produces a
headless, background PNG for a vision QA loop (Chromium screenshot for PDF;
isolated-profile `soffice --headless` for decks). The native lopdf/OOXML writers
are **kept as the fallback** when no renderer/browser is found — not deleted — so
document creation never hard-fails.

**Consequences:** the sidecar must be locatable: dev source (`bun run main.tsx`) →
compiled `dist/regent-cli` → `regent` on PATH, with `REGENT_CLI_PATH` overriding.
Browser via `REGENT_CHROMIUM_PATH`; LibreOffice via `REGENT_SOFFICE_PATH` (deck
previews degrade gracefully without it). CI stays green unchanged — renderer-backed
tests self-skip where no bun/browser exists, and the compiled binary bundles
PptxGenJS (both verified). One new Rust dependency: `tera`. Third-party files have
no manifest, so they cannot be edited in place — `read_document` + recreate.

# ADR-037 — Native document generation and the everyday toolset

Status: accepted · 2026-07-16

**Context:** Regent read office documents but couldn't write them — the
`documents` skill taught hand-rolled ZIP-of-XML. A Hermes Agent (MIT) port
review confirmed the gap and picked 8 portable skills.

**Decision:** `create_document` writes PDF (lopdf — printpdf rejected for its
unmaintained azul tree), DOCX (docx-rs), XLSX (rust_xlsxwriter), PPTX
(hand-rolled OOXML with designed layouts, subtitles, notes, and embedded raster
images). Bare deck names receive a slugged presentation folder. Ten everyday tools use
keyless APIs (Open-Meteo, dictionaryapi.dev, frankfurter.dev) or run offline;
`reminder` wraps the existing regent-cron store. All 11 ship deferred
(`tools.deferred`); the P4 gate re-based 2.2k→2.5k for the load_tools index.

**Consequences:** office round-trip is in-process; PPTX validity guarded by a
slideLayout-rels test (PowerPoint rejects without it — found via COM). The
`create_documents` / `everyday_live` examples are the standing runnable proofs.
The bundled skill requires a concise narrative and purposeful generated or
source-extracted images; the same tool path serves desktop, Butler, and CLI.

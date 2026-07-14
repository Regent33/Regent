# ADR-028: Constitutional prompt — always-on persona row, then tri-modal memory

**Status:** Accepted (2026-07-02) · **Amended 2026-07-08, reaffirmed 2026-07-14: always-on,
never disableable** (was opt-in; owner decision, non-negotiable for the main distribution)

**Context:** The prompt was one monolith (`BASE_PROMPT` + `CAPABILITIES`). The user mandates a
separate CONSTITUTIONAL_PROMPT (values/character/hard boundaries) that is accurate,
precise, and token-efficient — inserted into memory (Graph + SQLite + Vector + rank-fusion) at
setup rather than stuffed verbatim forever.

**Decision:**
- Three layers in `regent-agent::domain::prompts`: `SYSTEM_PROMPT` · `CONSTITUTIONAL_PROMPT`
  (versioned `prompts/constitution.md`, 16 numbered sections) · `CAPABILITIES`.
- The constitution lives in the `constitution` **persona row** — rendered first by
  `persona_block()` with a supremacy header, so deacon + gateway inherit it identically.
  **Always on:** the config loader forces `constitution.enabled = true` regardless of
  config.yaml, and `regent setup` states it rather than asking; the deacon seeds on boot
  (user edits to the persona row survive).
- **Phase 2 (vectorize):** sections become trusted graph nodes retrieved by the ADR-013
  tri-modal lane; the always-on row shrinks to a core. Safety-relevant sections (hard
  boundaries, crisis, minors) stay verbatim — limits must never depend on retrieval recall.

**Consequences:** Prompt bytes change → one-time prompt-cache invalidation per session model.
A shipped-document update won't auto-clear an older seeded copy on disable (reads as edited).

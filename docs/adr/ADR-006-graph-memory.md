# ADR-006: Graph memory in the session database; tool-mediated recall

**Status:** Accepted

**Context:** Hermes's cross-session memory beyond ~1.3k prompt tokens is lexical session search or
external provider plugins. Regent makes structured memory native (proposal §5).

**Decision:** One graph in the same SQLite file (schema v3): `nodes` (kind, provenance→trust
prior, TTL, access telemetry, dedup `content_hash`) + `edges` + `nodes_fts`. Persistence
primitives live in `regent-store`; all semantics in `regent-graph`: write policy (injection/
invisible-unicode scan, size caps), the bounded MEMORY/USER prompt stores (Hermes budgets, no
auto-compaction, frozen snapshot per session), hybrid retrieval (OR-of-prefixes FTS seeds →
1-hop expansion → RRF × trust × recency), and episode capture on compression. Recall reaches the
model via `memory_search`/`session_search` tools; results are rendered as provenance-quoted data,
never instructions. Embeddings stay eval-gated (the lexical+graph golden set already scores
recall@5 = 1.00, MRR = 0.79).

**Consequences:** No memory-provider plugin zoo needed for the default path; everything is
write-through (no flush-before-compression hazard); the golden eval is the regression gate for
any schema/scoring/sanitizer change.

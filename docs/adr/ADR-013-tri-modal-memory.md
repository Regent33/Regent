# ADR-013: Tri-modal memory retrieval (Graph + FTS5 + Vector)

**Status:** Accepted (2026-06-16)

**Context:** FTS5 + graph (M2) misses synonym/structural-paraphrase recall. User mandate (overriding
the P4 design's conditional embedding gate): add a vector lane and fuse all three so memory is
**superior and more token-efficient** than Hermes — higher precision@k means fewer, more on-point
nodes injected per turn. Must stay local-first and zero-cost.

**Decision:**
- **Three lanes, one ranker:** lexical (FTS5/BM25) + semantic (cosine over embeddings) fused by
  weighted reciprocal-rank, then graph 1-hop expansion, then `trust × recency`. Fusion is *additive*
  — no embedder ⇒ original FTS + graph behaviour (existing golden gate unchanged).
- **Local ONNX embeddings:** `all-MiniLM-L6-v2` (384-dim) via `fastembed` in an isolated
  `regent-embed` crate behind the kernel `EmbeddingProvider` trait. Offline after one model
  download; zero per-query cost; no PII leaves the machine.
- **Storage:** f32 BLOBs in `node_embeddings`, keyed by `model_id`; brute-force cosine in Rust
  (sub-millisecond at personal-agent scale — **no C ANN extension**; swappable to `vec0` later).
- **Reranking = the fusion + trust/recency pass.** A cross-encoder reranker is deferred until evals
  show precision demands it (YAGNI).
- **Graceful + migratable:** model load failure degrades to FTS + graph; a model change re-embeds via
  `nodes_needing_embedding` (dim-mismatched vectors are skipped, never mis-ranked).

**Consequences:** +`fastembed`/`ort` dependency (heavy build, network only on first model fetch).
Embed-on-write + background backfill in the daemon. Offline test suite stays FTS+graph; the
real-model paraphrase eval is `#[ignore]`-gated (network + model). Vectors are versioned, so a model
upgrade is a backfill, not a wipe.

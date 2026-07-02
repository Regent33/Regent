# P4 — Memory & Retrieval Completion Design

**Phase:** P4 · **Roadmap:** [next-steps.md §P4](next-steps.md) · **Impl:** `crates/regent-graph/`

---

## 1. Current retrieval architecture (M2 — implemented)

Pipeline: `query → stopword-strip → OR-of-prefixes FTS5 → BM25 seeds → bounded 1-hop graph expansion → reciprocal-rank × trust × recency score → top-k`

Key choices and their rationale:
- **OR-of-prefixes FTS5** over implicit-AND: prevents zero-hit failure on multi-word queries; stopword stripping eliminates false BM25 signal.
- **1-hop expansion**: BM25 seeds fetch semantically adjacent nodes via the `edges` table — graph topology provides the "meaning proximity" that vector search would otherwise supply.
- **Reciprocal-rank × trust × recency**: trust scores (`user_stated=1.0 / agent_inferred=0.7 / tool_output=0.4 / web_content=0.3`) bias retrieval toward high-confidence nodes; recency prevents stale data from displacing fresh.

**Regression gate** (`crates/regent-graph/tests/golden_retrieval.rs`): 12 query→expected-node pairs; **recall@5 ≥ 0.75, MRR ≥ 0.60** must pass on every schema/scoring/prompt change.

---

## 2. The embedding gate decision (sqlite-vec — conditional on P4 eval)

**Do not add vector embeddings unless the paraphrase eval class fails.**

| Eval class | Example | FTS5 coverage |
|---|---|---|
| Exact lexical | `"rust async runtime"` | ✅ strong |
| Prefix / stemming | `"async runtimes"` | ✅ OR-of-prefixes |
| Synonym substitution | `"concurrent executor"` → rust runtime node | ❌ potential miss |
| Structural paraphrase | `"how does the scheduler handle timeouts"` | ❌ potential miss |
| Negation / absence | `"no cron job scheduled"` | out of scope |

**Gate test methodology** (implemented as part of golden set expansion):
1. For each existing golden pair, generate one synonym and one structural-paraphrase variant.
2. Run the FTS5 hybrid pipeline against all variants.
3. If `recall@5 < 0.75` on the paraphrase sub-class → sqlite-vec embedding is justified.
4. If gate passes → defer indefinitely; re-run at every major schema change.

**If sqlite-vec is adopted:** add a parallel vector search path producing its own ranked seed list; fuse with FTS5 seeds via the existing reciprocal-rank merger (same scoring stage, new input lane). Embeddings stored in a new `node_embeddings` table (`node_id, model_id, vector BLOB`); model version tracked alongside `_config_version`.

---

## 3. P4 eval targets

| Target | Current | P4 goal |
|---|---|---|
| Golden pairs | 12 | ≥ 50 |
| Eval classes | exact + graph-hop | + synonym + paraphrase + time-sensitive + multi-entity |
| Trajectory evals | ❌ | ✅ (see below) |

**Golden set format** (extend `tests/golden_retrieval.rs`):
```rust
GoldenQuery { query: "…", expected_ids: &["node-a", "node-b"], min_recall_at_5: 0.75, min_mrr: 0.60 }
```

**Trajectory evals** — evaluate the review agent's full retrieval + memory decisions, not just final answers:
- Capture: `(session_fixture, expected_memory_writes, expected_retrievals_per_turn)`
- Gate: fraction of expected writes that land with correct provenance ≥ 0.80; retrieval path matches expected nodes in top-5 ≥ 0.75.
- Location: `crates/regent-graph/tests/trajectory_evals.rs` (new file, P4).

---

## 4. Write-approval staging

**Flow** (P4 addition, hooks into daemon's approval machinery from ADR-009/ADR-011):

```
BackgroundReview agent
  → memory write candidate
  → ApprovalQueue::enqueue(pending_write)   ← persisted in regent-store (new table)
  → daemon emits notification: memory.write_pending { id, content, provenance, trust }
  → user/CLI: /memory pending | approve <id> | reject <id>
  → on approve: GraphMemory::write(…) (same path as today)
  → on reject: discard, log to episode
```

`ApprovalQueue` domain contract (mirrors `ApprovalHandler` in regent-gateway): `enqueue → pending_list → resolve(id, approved: bool)`. Persistence: new `pending_memory_writes` table in `regent-store` (additive schema, same v-bump pattern). Auto-expire: pending writes older than TTL (configurable in config.yaml) are rejected automatically — never silently committed.

---

## 5. Episode-on-session-end (P1 daemon integration)

M1 compression already records episodes on context eviction. P1 daemon adds the **session-end path** (graceful shutdown and explicit `/new`):

```
SessionManager::drain(session_id)
  → Agent::end_session() — new method
  → if session has ≥ 1 turn AND no compression already fired this session:
      summarize last N turns via auxiliary model (same path as compression summarizer)
      GraphMemory::record_episode(session_id, summary)
  → BackgroundReview::join() — flush any in-flight review writes
  → drop SessionEntry
```

This ensures every session leaves a retrievable episode node regardless of whether compression triggered. The episode is tagged `source: session_end` (vs `source: compression`) so retrieval scoring can weight them independently in future.

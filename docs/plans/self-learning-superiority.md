# Making Regent's self-learning superior — evidence, gaps, plan

Status: **PROPOSED — awaiting approval.** Nothing here is implemented.
Date: 2026-07-27
Author: engineering session (log-driven investigation)

---

## 0. The correction that reframes this

An earlier claim in conversation — *"Regent is ahead: semantic retrieval by
default"* — was **wrong as stated**, and the plan below exists because of it.

Regent's always-on memory block is [`render_prompt_block`](../../src/crates/regent-graph/src/application/entries.rs)
(`entries.rs:93`). It renders **every** entry, joined by `\n§\n`, with a
char-limit header. The embeddings / FTS / graph lane is real, but it powers the
`memory_search` **tool** and the constitution — *not* the block every turn pays
for.

And the budgets ([`orchestrators.rs:34-35`](../../src/crates/regent-graph/src/application/orchestrators.rs)):

| | Regent | Hermes (`tools/memory_tool.py:167`) |
|---|---|---|
| memory cap | `2_200` chars | `2200` chars |
| user cap | `1_375` chars | `1375` chars |
| write past cap | refuse | refuse |
| injection | all entries, every turn | all entries, every turn |

Byte-identical. On the axis that matters most, **Regent and Hermes are the same
system with the same ceiling.** We ported their numbers along with their design.

### The ceiling is not theoretical — this machine is nearly full

Measured against the live store (`~/.regent/state.db`, `nodes`):

```
kind=memory   6 nodes   1654 chars   →  75% of 2200
kind=user     4 nodes    910 chars   →  66% of 1375
```

**Six memory entries and it is three-quarters full.** The next few writes start
failing with "would exceed the limit". This is a second, independent reason the
owner experienced "Regent doesn't remember": the reviewer was 402-ing (fixed in
`a159d0a`) *and* the store is about to jam.

Meanwhile `kind=constitution` holds **18 nodes / 13,764 chars** and is served by
retrieval, not injection — six times the memory corpus, at no per-turn cost.
The machinery this plan needs is already in production here. That is the whole
reason this is cheap.

---

## 1. Gaps in Hermes's self-learning

Established by reading the source, not from impression.

| # | Gap | Evidence |
|---|---|---|
| G1 | **Learning saturates at a fixed cap.** Memory is a 2200-char notepad; a write that would exceed it is refused. Past the cap, learning requires eviction, and nothing ranks entries by value. | `tools/memory_tool.py:393,420,436` |
| G2 | **Whole-corpus injection.** `MEMORY.md` + `USER.md` load into the system prompt every turn regardless of relevance to the turn. | `agent/agent_init.py:1598-1620` |
| G3 | **No usefulness feedback.** Nothing records whether a saved lesson was ever retrieved or helped. Curation cannot prefer what paid off. | no hit counter in `memory_tool.py` |
| G4 | **Lexical-only learning graph.** `learning_graph.py` derives memory↔skill edges from *lexical overlap*, so paraphrases don't connect. | `agent/learning_graph.py:5-10` |
| G5 | **Reviews are lossy.** `_spawn_background_review` is wrapped in `except Exception: pass` with no durable cursor — a dropped review is gone. | `agent/turn_finalizer.py:651-657` |
| G6 | **Review cost scales with turns.** Fires after every qualifying turn; full-transcript replay on the same model. | `agent/background_review.py:34-45` |

**Hermes's genuinely better idea, which we lack:** an explicit *"Do NOT capture"*
list (`background_review.py:271-292`) — environment-dependent failures, transient
errors that resolved, one-off narratives, and above all **negative tool claims**:

> *"'browser tools do not work', 'X tool is broken'. These harden into refusals
> the agent cites against itself for months after the actual problem was fixed."*

Regent's `REVIEW_SYSTEM_PROMPT` has no equivalent. Given today's logs (45 browser
MCP failures, 276 rate-limits), we are actively exposed to persisting exactly
those false constraints.

### Which gaps Regent shares

G1, G2, G3 in full (same caps, same injection, no hit counter). G5 we already
beat — `reviewed_message_count` advances only after success, so an interrupted
review retries. G6 we already beat — batched at `min_new_messages: 8` behind a
serializing gate. G4 we already beat — real embeddings, just not on this path.

---

## 2. Design

The one insight that makes this both cheaper and unbounded:

> **Stop conflating "what the agent knows" with "what is in the system prompt."**

Today they are the same set, so the prompt budget *is* the learning budget. Split
them and both problems dissolve — tokens drop *and* the ceiling lifts.

### The cache constraint (why the obvious version is wrong)

The naive fix — retrieve top-K per turn into the system prompt — is a trap. The
prompt is a Tier-1 ledger segment frozen at session build (`build.rs`,
`Segment::tier1("memory", …)`). Recomputing it per turn would bust the stable
prefix **every turn**, which is precisely the regression SPL telemetry exists to
catch. It would trade ~600 tokens of memory for a full-price prompt each turn:
a large net loss.

So the split is by *stability*, not by relevance alone:

- **Tier-1 (system prompt, stable):** pinned + identity/preference memory only.
  Small, changes rarely, stays cache-warm. This is what must be true regardless
  of what is asked.
- **Per-turn (a retrieved context message, NOT the prompt):** relevance-ranked
  entries for *this* turn. Rides outside the cached prefix, so it costs its own
  tokens and nothing else's.

`memory_search` already does the retrieval. `pinned` already exists
(`memory.pin`/`memory.unpin` RPCs). Both halves exist.

---

## 3. Phases

### P1 — Split the memory block by stability *(core)*
- `render_prompt_block` renders **pinned + `user` profile only** → Tier-1 stays
  small and stable.
- A new per-turn retrieval injects top-K relevant unpinned memory as context.
- Net per-turn tokens: strictly lower once the corpus exceeds the pinned set,
  and it stops growing with the corpus.

### P2 — Turn the hard cap into a curation trigger *(core)*
Once the prompt no longer carries everything, a refuse-at-2200 write serves no
one. Past a **soft** limit the reviewer is asked to consolidate/merge/evict by
value; the tool stops refusing. Learning becomes unbounded; prompt cost stays
bounded by P1.

### P3 — Port the anti-lesson guardrails *(cheapest, highest immediate value)*
Add the negative list to `REVIEW_SYSTEM_PROMPT`: no environment-dependent
failures, no negative tool claims, no transient errors that resolved, no one-off
narratives. ~15 lines of prompt. Do this **first** — it is the one thing Hermes
is unambiguously better at, and it prevents damage that is expensive to undo.

### P4 — Usefulness feedback *(beyond both systems)*
Record a retrieval hit-count + last-hit timestamp per node. Curation (P2) then
evicts by *(hits, age)* instead of insertion order. Neither Hermes nor Regent
knows today which lessons ever paid off; this is where we pass them rather than
catch up.

### P5 — Semantic contradiction handling
`add_node` dedupes by content **hash** — a paraphrase slips through and a
*changed* preference appends alongside the stale one. Extend to near-duplicate
(cosine above threshold) → replace instead of append.

### P6 — Benchmark harness (Hermes is installed locally)
Three measurements, both agents, same corpus:
1. **tokens in the memory path per turn** (ours: ledger telemetry already
   reports Tier-0/Tier-1 hashes + usage; theirs: count the injected block).
2. **recall@k** on a fixed question set against a seeded corpus.
3. **writes accepted before saturation** — expected: Hermes fixed, Regent
   unbounded after P2.

---

## 4. Files (gate: > 3 files)

| File | Change | Why |
|---|---|---|
| `regent-skills/src/application/prompts.rs` | add the do-not-capture list | P3 |
| `regent-graph/src/application/entries.rs` | `render_prompt_block` → pinned + user only | P1 |
| `regent-graph/src/application/orchestrators.rs` | soft limit; stop refusing | P2 |
| `regent-agent/.../turn/` (one seam) | inject retrieved per-turn memory | P1 |
| `regent-graph/src/application/…` (retrieval) | hit counter + eviction ranking | P4, P5 |
| `regent-store` schema | `hit_count`, `last_hit_at` columns (additive, `RECONCILE_COLUMNS`) | P4 |
| tests alongside each | pure-domain first | contract §7 |

**Risks.** (a) Tier-1 bytes change → SPL prefix tests will need rebaselining
deliberately, and that is the point of those tests, so each change must be
argued not silenced. (b) A memory the model *needs* but retrieval misses — why
pinned + user profile stay unconditional. (c) P2 relaxes a safety limit; the
prompt bound must land (P1) *before* the cap loosens (P2), never the reverse.

**Recommended first slice:** P3 alone (one file, immediate, no contract change),
then gate again for P1+P2 together — they must ship in that order.

---

## 5. Verification

Repo's own gates: `cargo test --workspace --exclude regent-voice-server`,
`cargo clippy --all-targets`, `cargo fmt --check`; Desktop `bun run typecheck`,
`bun test`, `bun run build`. Plus P6's measured numbers before/after, and a live
check that the store accepts writes past 2200 once P2 lands.

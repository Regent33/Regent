# Regent superiority plan — all seven audit dimensions

Status: **P0 is a live regression. P3 shipped (`d1270d2`). Everything else PROPOSED.**
Renamed from `self-learning-superiority.md`, which covered one dimension.
Inputs: `docs/research/hermes-vs-regent-comparison.md` (external audit — Hermes 41 /
Regent 10 / 13 ties) plus this session's log and source investigation.

Goal: win every criterion in the audit, or state plainly why a criterion is not
winnable by engineering inside this repo.

---

## P0 — Regent has had no shell. Fix before anything else. *(my regression)*

`64aad1f` "jail every session to its cwd by default" (Jul 26 11:34) made
`should_sandbox` default-on for local sessions, without accounting for
`terminal.rs:79`, which refuses a local shell in **any** sandboxed context. That
guard was written for untrusted ingress; applied to every session it is a blanket
ban: **no `npm install`, no build, no test, no verify, in any ordinary session.**

Not inference — Regent authored `jailed-terminal-fallback` and
`npm-background-build-verification` to cope. The first records **five** recurrences
of one job (`molecular-biology-site`, task_ids 99/107/165/173/172), each ending
"I'll report back" with nothing delivered, and notes that `background_task`
**inherits the same jail**, so the escape hatch was never real.

One cause behind four separate reports: low-quality coding output; "still running"
after completion (the `a159d0a` dedupe/staleness fix treated the symptom);
`background_task` doom loops; and skills encoding "terminal doesn't work" — the
exact anti-lesson class P3 blocks, written before P3 landed.

**Fix.** Distinguish *why* a context is jailed, on `ToolContext`, set where
`should_sandbox` already decides:
- external ingress or explicit `REGENT_SANDBOX` → keep refusing the local shell;
- the default local hygiene jail (`64aad1f`) → allow it. File-path containment is
  unchanged either way.

Security-posture line → **owner confirmation required.** The status quo is not the
safe option; it is a non-functional coding product that poisons its own memory.

**P0b** — archive (never delete) the two skills above once P0 lands; they encode a
constraint that becomes false. Re-check memory/persona for the same claim. Owner's
data → owner's call.

**P0c** — a test asserting an ordinary session can run a command. The regression
shipped with a fully green suite because nothing covered that.

---

## 1. Audit corrections

The audit is careful and mostly right. Five items are wrong or stale and must not
drive priorities.

| Audit claim | Finding |
|---|---|
| #4 "no dependency-scanning workflow — largest security-process gap" | **Wrong.** `ci.yml:39-52` runs `cargo-audit` + `cargo deny check` as a dedicated `supply-chain` job on every run. |
| #5 "terminal → artifacts-jail bypass: known, flagged, unfixed" | **Stale.** Fixed at `terminal.rs:73-86`. The live defect is the inverse (P0): over-applied. |
| #2 "split memory by stability (pinned + user profile)" | **Rests on my error**, which the audit inherited. Pinning is `ttl_expires_at IS NULL` and every entry node already has none (`memory` 6/6, `user` 4/4) — the filter selects everything. |
| Self-learning (a) + (e): `/learn` credited to Hermes | **Wrong.** Regent has `/learn` (`prompt_ops.rs:37-46`) — one command turns a directory, URL, or the conversation itself into a skill. Two criteria scored against a capability we ship. |
| Performance (f): "local + Docker" only | **Wrong.** `parse_backend` supports **local, docker, ssh**. |

Holding, and uncomfortable: our per-turn memory path **is** byte-identical to
Hermes's, and **zero** of the four performance criteria is benchmarked on either
side.

Adjusted honest baseline: roughly **Regent 12 / Hermes 39 / 13 ties**.

---

## 2. Track A — Measurement first *(promoted; unblocks every claim below)*

Nothing in the audit's Performance or Coding-correctness rows is measured, on
either side. Hermes is installed locally, so this is available today.

1. **Memory-path tokens/turn** — ours from the ledger telemetry that already
   reports Tier-0/Tier-1 hashes and usage; theirs by counting the injected block.
2. **recall@k** on one seeded corpus, both systems.
3. **writes-before-saturation** — expected: Hermes fixed at 2200, Regent unbounded
   after P2.
4. **SWE-bench subset** — Hermes ships `mini_swe_runner.py`; run the same subset
   against `regent code` so *correctness* stops being an inference for both.

Without A, the rest of this plan is opinion.

---

## 3. Per-dimension tracks

### D1 · Memory — *already ahead (5/2/4); convert the rest*
| Criterion | Move |
|---|---|
| (c) recall speed *(tie)* | ANN index. Already documented as "swappable to vec0"; brute-force O(N) cosine is the only thing between us and a clear win. |
| (f) update & consistency *(tie)* | **P5** — semantic near-duplicate replace. `add_node` dedupes by content *hash*, so a paraphrased preference change appends beside the stale one. |
| (h) contextual association *(tie)* | Hermes's episodic session search is genuinely better UX (±5-message windows, bookends, re-anchoring, zero LLM cost). Port the windowing onto our existing `session.search`. |
| (i) generalization *(Hermes)* | Follows D2's curator + skill quality, not a separate build. |
| (j) privacy *(Hermes)* | Scan memory **writes** for injection patterns before staging — see D5, one shared library serves both. |
| — | **P1** (usefulness split) and **P2** (cap → curation) below: prompt cost stops growing with the corpus, and learning stops hard-stopping at 2200. Store is at 75% with six entries. |

**P1 — split the memory block by usefulness, not pin state.** `access_count` is
live and informative (270 hits across 6 memory nodes, 202 across 4 user nodes) and
`retrieve.rs` already scores with recency decay.
- Prompt block: top-N by `(access_count, recency)` under a small fixed char budget.
- Per-turn: `retrieve(query, k)` + `render_recall(..)` — **both already exist and
  already serve `memory_search`** — injected outside the cached prefix.

**Ships whole or not at all.** Narrowing the prompt block alone makes every entry
outside the top-N reachable only if the model chooses to call `memory_search` — a
recall regression dressed as an optimization.

*Open decision, do not default:* where the per-turn block goes. The dispatcher's
decoration seam (`wrap_prompt`, `editor_note`) is cheapest but lands in stored
history, so it needs a `promptDecorations` stripper like attachments got. A seam in
`regent-agent`'s turn assembly keeps history clean but touches the hottest path.

**P2 — hard cap → curation trigger. Must land after P1, never before.** Past a
*soft* limit the reviewer consolidates/merges/evicts by value; the tool stops
refusing writes. Loosening the cap while the prompt still injects everything would
grow every prompt without bound.

### D2 · Self-Learning — *the decisive loss (2/7/1); winnable*
| Criterion | Move |
|---|---|
| (b) accuracy | **SHIPPED** (`d1270d2`) — anti-lesson guardrails. |
| (a), (e) speed / sample efficiency | **Already ours** — `/learn` exists; audit mis-scored. No work. |
| (f) feedback utilization | **P4** — `access_count` *and* `last_accessed_at` already exist on `nodes` and are already incremented on read (`graph.rs:165`). No schema change. Evict by `(hits, age)`. |
| (g) adaptability, (h) autonomy | **Skill curator.** `.usage.json` already records `use_count`/`view_count`/`state` per skill (`library.rs:92-176`) — and **nothing consumes it**. A curator that archives dormant skills, promotes proven ones, and patches ones that failed in use closes both criteria off data we already collect. Biggest self-learning win available. |
| (d) generalization | Follows (g) plus P3's authoring standards. Skill *count* (16 vs 180) is surface area, not capability — §5. |

With P3 + P4 + curator this dimension flips from 2/7 to roughly 7/2.

### D3 · Performance — *split; make it measured*
| Criterion | Move |
|---|---|
| (a),(b),(e),(g) | Track A. Our advantages are real but **inferred from language choice** until benchmarked. |
| (c) throughput | The background-task board is a process-global `static Mutex<Vec<Task>>` with a `ponytail:` note to shard later. "Later" is when multi-tenant matters — not now. |
| (f) scalability | We have local/docker/ssh. Serverless-hibernating backends (Modal/Daytona) are a genuine gap; low priority for a single-operator product. |
| (h) error recovery | **Rate-limit tracker.** Today's logs: **276 HTTP 429s** and ~465 failover events. A tracker that learns per-provider limits and paces requests is worth more here than any module count. |

### D4 · Automations — *widest gap (0/8); mostly additive*
| Criterion | Move |
|---|---|
| (a),(b) reliability / completion rate | **Executions ledger.** `regent-cron` has **no** execution history (verified). Recording each run's start/end/outcome makes completion rate *observable* — the precondition for the metric, and the cheapest item here. |
| (c) retry | Retry with backoff + dead-target detection on the ledger. |
| (d) flexibility | Automation *suggestions*: propose cron jobs in natural language from observed repetition. Novel, not a port. |
| (f) monitoring | A runs view over the ledger (Desktop already has the surface). |

### D5 · Security — *(0/7/1); one flagship, one correction*
| Criterion | Move |
|---|---|
| (c) prompt-injection | **Shared threat-pattern library**, applied to memory writes, tool results, and context files. We render recall inert with provenance labels but **never scan**. Flagship; also wins D1(j). |
| (d) sensitive data | `secret_scope` on top of the existing kernel `redact`. |
| (f) audit | Delivery + execution ledgers (shares D4's work). |
| (a) access control | Gateway auth exists; device pairing / dashboard auth are gateway-maturity items. |
| (g) exploit resistance | **Already ours** — audit wrong (#4). |
| (e) action safety | Ours already: ephemeral per-command container, deny-all voice, plan→verify→revert default. |

### D6 · Orchestration — *(1/6/2)*
| Criterion | Move |
|---|---|
| (d) coordination | `max_concurrent_children` cap + spawn pause. |
| (f) fault tolerance | Active-child registry + `interrupt_subagent`. |
| (h) aggregation | Real result aggregation. `background_task` is fire-and-acknowledge — prepending a trimmed result to the next turn is not aggregation. |
| (a),(c),(g) | Follow from the above; our `agents` table (name + role + prompt + model + **tool allow-list**) is the cleaner foundation of the two — the audit says so. |

### D7 · Coding — *(0/7/3); highest ceiling, largest effort*
| Criterion | Move |
|---|---|
| (d) context, (c) debugging, (f) refactoring, (g) multi-file | **LSP client.** The audit's strongest technical finding: real semantic context vs text search. One investment moves four criteria at once. Its own project, after P0/P1. |
| (a) correctness | Track A.4 (SWE-bench subset). |
| — | Ours to keep: **plan → verify → revert as the default path**, which the audit notes Hermes does not enforce by default. |

---

## 4. Order

1. **P0 / P0b / P0c** — restore the shell. Nothing else matters while it holds.
2. **Track A** — measurement, so everything after is verified rather than argued.
3. **P1 + P2** (that order, together).
4. **D2 curator** + **P4** — flips the decisive dimension off data we already collect.
5. **D4 executions ledger** + **D3 rate-limit tracker** — cheap, and both already-felt pain.
6. **D5 threat-pattern library** (wins D5c + D1j).
7. **P5**, **D1 ANN index**, **D1 episodic windowing**.
8. **D6 orchestration**, then **D7 LSP** as a scoped project.

## 5. What engineering in this repo cannot win

Stated so the scorecard is not chased dishonestly:

- **Test mass** (21 K vs 889 K LOC), **contributors** (1 vs ~108), **skills**
  (16 vs 180), **platforms** (18 vs 26), **provider plugins** (19 vs 33). Surface
  area produced by an institution over time. Real advantages for Hermes; they do
  not close in a sprint. Growing skills/platforms on demand is reasonable;
  targeting the *counts* is not.
- **Institutional security process** — live advisory handling with real reporter
  traffic. We can match mechanisms, not the process.

The honest target: **win every capability criterion, concede the surface-area ones,
and be measured about both.**

## 6. Verification

Repo gates: `cargo test --workspace --exclude regent-voice-server`,
`cargo clippy --all-targets`, `cargo fmt --check`; Desktop `bun run typecheck`,
`bun test`, `bun run build`.

P0 additionally needs a **live** check (open a real repo, run `npm --version`
through the terminal tool) plus P0c's regression test. Track A supplies the numbers
for D1–D3 and D7(a); no claim from those rows ships as fact without them.

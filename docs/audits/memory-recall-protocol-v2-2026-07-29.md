# Frozen protocol v2 — paired memory measurement

**Status: FROZEN.** Committed before the v2 harness was written and before any
v2 result existed. Same rule as v1: if a commit touching this file is newer than
the commit carrying v2 results, the run is void.

v1 (`memory-recall-protocol-2026-07-29.md`) produced one score and four
problems. This replaces its metrics; it does not amend them. v1's result stands
as written.

---

## 1. What v1 got wrong, and the fix

| v1 problem | v2 fix |
|---|---|
| `recall@k` compared Regent's *ranking* against Hermes's *file order* | Drop `@k` comparison entirely. Measure each system's **natural delivery**, plus whether position carries any relevance signal at all (§4). |
| `precision@k` was denominator arithmetic once recall hit 1.0 | Replace with **waste** and **cost per relevant entry** — absolute, not normalised by a fixed k. |
| Gold-first ordering meant saturation only ever dropped distractors | **Randomised insertion order**, 3 seeds, gold distributed throughout. |
| N=60 and N=200 stored identical sets — duplicate conditions | N chosen so stored counts actually differ (§3). |
| `0.000 ms` backend microbenchmark | Measure **per-turn context cost**, which is what the user pays. |
| "tie" invented after seeing 31 v 30 | **Equivalence margin predeclared below.** |

## 2. Predeclared rules

1. **Equivalence margin.** For count metrics, a difference of **≤ 2 entries or
   ≤ 5%** (whichever is larger) is a **TIE**. Declared now, applied blind.
2. **N/A rule.** If a system does not implement the operation a criterion names,
   that criterion is **N/A for cross-system scoring**, not a win for the other.
   It is still reported as a system fact. *Hermes's built-in memory has no
   search method; this rule is expected to fire on any ranking criterion.*
3. **Seeds.** 3 fixed seeds (11, 22, 33). Every metric is the mean across seeds;
   if seeds disagree across an anchor boundary, the criterion stays UNSCORED.
4. **No metric may be changed after a result is seen.** If a metric proves
   malformed, the criterion is retired as UNSCORED with the reason — it is not
   swapped for a kinder one.

## 3. Corpus and conditions

- Same entry style as v1 (≤120 chars, single line). **New corpus**: gold
  redistributed and distractor queries rewritten (§5).
- **N ∈ {10, 20, 30, 200}.** The first three sit below, at and just past the
  ~30-entry saturation point so stored counts genuinely differ. N=200 is one
  condition testing saturation behaviour, **not** three.
- Insertion order is a seeded shuffle of the whole corpus, so the cap drops gold
  and distractors alike — the failure v1 never reached.

## 4. Metrics

Per system, N, and seed:

- **stored** — entries accepted (criterion d).
- **delivered_recall** — |gold ∩ delivered| / |gold|, over each system's
  *natural* delivery: Regent's ranked top-10, Hermes's entire block. No
  artificial truncation of either.
- **waste** — delivered entries that are not gold, as a count and as chars.
  Criterion (e) is about narrowing, so the absolute noise is the measure.
- **rank signal** — for each query, the position of the first gold entry in the
  delivered list. A system whose position is independent of the query has **no
  relevance signal**, which is testable: compare the delivered order across
  queries. Identical order for every query ⇒ signal = none. This is the honest
  form of criterion (e) and it does not depend on k.
- **context_chars_per_turn** — memory characters entering the prompt on an
  ordinary turn (criterion c, reframed): Hermes's block is unconditional;
  Regent's is its frozen block plus retrieval only when the tool is called.
  **Both components reported separately; no single number is scored.**

## 5. Query set

30 queries, regenerated. Two changes from v1:

- **Distractor queries must share ≤ 2 content tokens with their gold entry**
  (v1's averaged 4.3, which made them lexically easy). Verified by the builder
  before the run; it fails loudly if the constraint is violated.
- Gold entries are spread across the corpus, not clustered in the first 20.

## 6. Anchors

Unchanged from v1 §6, plus: a criterion that fires the N/A rule is recorded
**N/A**, and a criterion whose seeds disagree across a boundary is **UNSCORED**.

## 7. Predictions

1. **d** — TIE under the new margin. Both cap at 2,200 chars; v1 measured 31 v
   30, inside ±2.
2. **delivered_recall** — near-identical, because with the same insertion order
   both admit nearly the same set. If they differ by more than the margin,
   suspect the harness.
3. **waste** — Regent materially lower (≈9 non-gold per query vs ≈29). This is
   the one I expect to separate them.
4. **rank signal** — Regent non-trivial (v1: gold at rank 1 for 23/30);
   Hermes exactly zero by construction. Expected to fire the N/A rule for any
   ranking criterion, which is a *finding*, not a Regent win.
5. **c** — Hermes's unconditional per-turn cost exceeds Regent's frozen block;
   Regent's retrieval cost applies only on tool-calling turns. I expect no clean
   single-number winner and (c) to stay UNSCORED.

If prediction 3 fails — if Regent's waste is not materially lower — then
Regent's retrieval buys nothing over whole-corpus injection at this scale, and
that is the result.

## 8. Artifacts

`memory-recall-v2-2026-07-29/`: corpus, queries with gold, per-seed raw output
from both systems, harnesses, and the co-review. Every number in the result must
be reconstructible from these alone.

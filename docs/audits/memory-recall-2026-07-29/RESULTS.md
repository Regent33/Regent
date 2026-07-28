# Result — paired memory-recall pilot, 2026-07-29

**Outcome: one criterion scored, three unscored, and the pilot's own design
found to be flawed.** That is a real result, not a failed run: the §0 gate
exists to stop exactly the write-up I was about to produce.

Protocol: `../memory-recall-protocol-2026-07-29.md` (frozen `e609997`, before
the harness existed). Pre-run note: `PRE-RUN-NOTE.md` (`174097a`). Raw runs and
harnesses: `runs/`. Adversarial review by GPT 5.6 sol (read-only, no tools).

---

## 1. What was measured

| N | Regent stored | Hermes stored |
|---|---|---|
| 20 | 20 / 20 | 20 / 20 |
| 60 | **31** / 60 | **30** / 60 |
| 200 | **31** / 200 | **30** / 200 |

Both cap the entry path at 2,200 chars — verified in each source, byte-identical
(`orchestrators.rs:34` / `memory_tool.py:167`).

Regent, N=200, over its ranked top-10:

| metric | value |
|---|---|
| recall@1 | 0.750 |
| recall@3 | 0.967 |
| recall@5 | 0.967 |
| recall@10 | 1.000 |
| MRR | 0.871 |
| median latency | 4.5 ms |

Hermes returns a **byte-identical, file-ordered list for every query** and every
gold entry is in it. Median latency 0.000 ms — below timer resolution, so an
upper bound rather than a measurement.

## 2. Scores

| Criterion | Score | Why |
|---|---|---|
| **d** storage capacity | **Regent 4** | Under the literal frozen metric ("entries the system accepted"), Regent wins at 2 of 3 N. See the caveat below. |
| **b** recall accuracy | **UNSCORED** | Metric inapplicable across these architectures — not a tie. |
| **e** relevance filtering | **UNSCORED** | The measured number is denominator arithmetic. |
| **c** recall speed | **UNSCORED** | The two timed operations are not the same operation. |

### (d), and the caveat I am not allowed to apply

31 vs 30 is one entry, and I initially wrote it up as a tie. That was wrong:
**no practical-equivalence margin was predeclared**, so inventing one after
seeing the numbers is precisely the move the gate forbids. Under the frozen
metric Regent wins. My frozen prediction 2 said Regent would win (d) — it was
*not* disconfirmed, and the pre-run note's guess that it would be was itself
premature.

The honest reading of the mechanism is still that the systems have essentially
the same nominal capacity and the one-entry gap is accounting. A v2 must
predeclare an equivalence margin.

### Why (b) is unscored rather than tied

The frozen metric was recall@k. Under it Regent wins (1.000 vs 0.750 at k=10).
But the diagnostic showed Hermes's shortfall is **file-position luck**: it
returns the same list for every query, and the 7 "misses" are exactly the
queries whose gold sits at file position 11+.

So recall@k measures ranking for one system and storage order for the other.
Neither "Regent wins" nor "tie" is defensible — the comparison is malformed.
Swapping in a different metric ("recall over everything reaching the model",
where both score 1.000) after seeing the result would be changing the frozen
metric to rescue it, which is the same sin as the (d) margin.

### Why (e) is unscored

Precision here is almost entirely the denominator. There are 31 gold
memberships across 30 queries:

- Regent always emits 10 items → 31/(30×10) = **0.103**, its arithmetic ceiling.
- Hermes emits 20 or 30 → **0.052** / **0.034**.

Once recall is 1.0, these numbers measure output-set size, not ranking. What
they *do* support is a **context-density** statement, which is not criterion (e):

> Regent delivers about one relevant entry per ten supplied; Hermes about one
> per twenty to thirty, in every turn's prompt whether or not memory is relevant.

### Why (c) is unscored

`0.000 ms` is below timer resolution, not zero. And the operations differ in
kind: Regent embeds the query and ranks; Hermes returns a pre-rendered snapshot
string whose rendering happened outside the timed region. As an end-to-end
criterion it would have to include Hermes's per-turn prompt cost for a block it
injects unconditionally, and Regent's cost only on turns that call the tool.
Neither was measured.

## 3. The design flaw, which is the most useful thing this run produced

All 15 gold entries sit in the first 20 so the gold set is constant across N.
That choice interacts with the 2,200-char cap in a way I did not foresee:

**Once the store saturates at ~30 entries, raising N adds only refused writes —
and every refused write is a distractor.** The cap therefore *protects* recall.
"Robustness as N grows" was never tested; the systems simply stopped admitting
the hard cases. The predeclared "refused gold counts as a miss" rule never fired
because corpus order guarantees gold is accepted before saturation.

For the same reason **N=60 and N=200 are duplicates**: identical stored sets,
identical results. They are not two scale conditions, so counting them as two
wins for the "≥2 of 3 N" anchor double-counts one measurement. (d)'s score
inherits that weakness.

## 4. Harness audit — the trap the protocol set for me

The protocol said: *"If (1) shows Regent winning at N=20 as well, suspect the
harness before believing it."* It did. My first response was to diagnose
**Hermes** and stop, which does not discharge the condition. Run properly
against Regent:

| check | result |
|---|---|
| output varies by query | 30 distinct lists of 30 — not a constant |
| gold rank distribution | 23 at rank 1, 6 at rank 2, 1 at rank 7 |
| gold ids leak into indexed text | none |
| query↔gold token overlap | lexical 1.45, paraphrase 1.00, **distractor 4.30** |

Regent's retrieval is genuine: recall@1 is 0.750, so recall@10 = 1.000 is not
vacuous. **But the distractor queries carry 4.3 shared content tokens with their
gold** — over four times the paraphrase set. They are lexically easy, so the
"distractor-heavy" class did not test what its name claims.

## 5. What a v2 must change

1. **Randomize insertion order** across several seeds, with gold distributed
   throughout — so saturation drops gold and distractors alike.
2. **Predeclare an equivalence margin** for count-based criteria.
3. **Choose N values that differ after saturation**, or measure the retrieval
   path that is genuinely unbounded rather than the capped entry path.
4. **Rewrite the distractor queries** to share few content tokens with gold.
5. **Report end-to-end memory cost per turn**, not backend microbenchmarks, if
   (c) is to be scored at all.
6. Decide, in advance, whether a criterion whose metric one system cannot
   meaningfully implement is scored **N/A** rather than contested.

## 6. Correction to the review

5.6 sol's point 8 said recall@5 and precision@5 were missing. They were
measured; I omitted them from the review prompt. Regent's recall@5 is 0.967 at
N=60 and N=200 — the substance of the point (report both cutoffs) stands, and
they are in §1 and `runs/`.

## 7. Standing

Of the memory family's 11 criteria: **1 scored (d, Regent 4), 3 unscored with
reasons, 7 untouched.** The source audit's memory verdict is *not* refuted —
what this run establishes is that four of its rows rest on criteria that are
either architecturally incomparable or inadequately measured, which is a
different and weaker claim than "the audit was wrong".

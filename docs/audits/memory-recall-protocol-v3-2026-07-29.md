# Paired memory measurement — protocol v3

**Frozen before the harness exists.** Git order is the proof: this file is
committed before `build_corpus.py`, before `queries.json`, and before any run.
Nothing below may change once a number has been seen. Two previous rounds were
demolished in adversarial review for exactly that, both times by me swapping a
frozen metric for a friendlier one after the result was inconvenient.

Supersedes v1 (`e609997`) and v2 (`fba3355`). Both remain in the tree; neither
is deleted, because a withdrawn measurement is evidence about the measurer.

---

## 0. What v2 concluded, and why v3 exists

v2 ended with **0 of 11 memory criteria cleanly scored**, and the reason was the
finding:

> at a corpus size where one system simply injects everything, retrieval quality
> and whole-corpus injection are not comparable on these criteria. A meaningful
> comparison needs a regime where Hermes *cannot* inject everything — which
> means raising or removing the cap, not enlarging the offered corpus.

v3 builds that regime.

### A correction to my own framing

Closing v2 I wrote that raising the cap "modifies third-party software's
behaviour rather than measuring it as shipped", and treated it as an owner
decision on those grounds. **That was wrong, and checking the source is what
showed it.** The cap is not a constant:

```python
def __init__(self, memory_char_limit: int = 2200, user_char_limit: int = 1375):
```

It is a constructor parameter with a default. Passing a different value is
configuration through Hermes's own public API — the same surface any embedder
uses — not a patch, not a fork, not a modification. The v2 caution was
overstated and is withdrawn here rather than quietly dropped.

Both systems are therefore run at **their shipped defaults and at raised caps**,
and the default arm is always reported.

---

## 1. The regime v3 measures

v1 and v2 both compared **storage-bound** systems: the cap bit first, so what
they measured was capacity wearing a retrieval costume.

v3 removes capacity as a confound *on purpose*:

> **Raise both caps until the entire corpus is stored by both systems.** Then the
> only remaining difference is what each system **delivers** out of an identical
> store.

That is the isolation v1 and v2 never achieved. Hermes delivers its whole block;
Regent delivers a ranked subset. With storage equalised and delivery budgets
matched, criteria (b) and (e) finally measure retrieval rather than the cap.

**Arms** (all run, all reported):

| arm | memory cap | what it answers |
|---|---|---|
| `shipped` | 2,200 / 1,375 (Hermes defaults) | how the systems behave as installed |
| `raised` | large enough that the whole corpus fits both | retrieval with capacity neutralised |

Regent's own cap is raised to match in the `raised` arm. Neither system gets a
cap the other does not.

---

## 2. Corpus and queries

- **500 entries.** Large enough that the `shipped` arm is genuinely
  storage-bound and the `raised` arm genuinely is not.
- **20 gold entries**, positions spread across the whole corpus (not clustered),
  emitted by a builder that asserts the spread.
- **Hard negatives are the v3 addition.** v1's distractors shared 4.30 content
  tokens with gold — far too easy. v2 overcorrected to 0.30, which is
  *lexically* dissimilar and therefore not hard either: a system can win on word
  overlap alone. v3 requires, per gold entry, **at least 3 distractors that are
  semantically adjacent but factually wrong** — same entity, same topic, wrong
  value or wrong polarity ("prefers X" vs "no longer prefers X" vs "prefers X's
  competitor"). The builder asserts this count. Lexical overlap is *not*
  constrained, because constraining it is what made v2's negatives easy.
- **3 seeds** (11 / 22 / 33). Insertion order is a seeded shuffle of the whole
  corpus, emitted once to a file and read by **both** harnesses, so the
  sequences are byte-identical.

---

## 3. Metrics — frozen

### 3.1 Primary: `recall_at_budget`

For a delivered-context budget **B tokens**, each system produces its natural
delivery, truncated to B by its own natural order:

- **Regent** — ranked results, taken in rank order until B is exhausted.
- **Hermes** — the injected block, taken in block order until B is exhausted.

> `recall_at_budget(B)` = |gold ∩ delivered(B)| / |gold|

Budgets: **B ∈ {250, 500, 1000, 2000, 4000}**, reported as a curve.

This is the metric v2's review demanded and v2 did not have. It is policy-free:
neither system's `k`, block size, or delivery convention enters it, because both
are given the same number of tokens and asked what they put in them. The v1/v2
"waste" metric — which I chose `k` for — is **not** a scored metric in v3. It
may be reported as description only, labelled as such.

### 3.2 Secondary: `entries_stored`

Count per arm. Interpreted as capacity in the `shipped` arm; expected to be
identical in the `raised` arm and reported as a **sanity check** that the arm
did what it claims.

### 3.3 Secondary: `mrr`

Reciprocal rank of the first gold entry in the delivered order. Reported for
both. Undefined (not zero) when no gold was delivered; the count of defined
seeds is printed beside every mean.

### 3.4 Explicitly NOT metrics

- **"Rank signal" / distinct delivered orders.** v2's review was right: it
  detects query-*sensitivity*, not relevance. A hash of (query, doc) scores
  perfectly. Retained only as a **harness audit** line, never scored.
- **Latency.** One side was measured in v1 at below timer resolution and the two
  timed operations were not the same operation. Not scored in v3 either. Saying
  so in advance is the point.

---

## 4. Uncertainty — frozen

Every reported mean is accompanied by:

- the **per-seed values**, printed, not summarised;
- the **count of seeds where the metric is defined** (NaN is never averaged in —
  that bug blanked a whole row in v2 and made the table read as impossible);
- the **min–max range** across seeds.

**No confidence intervals are claimed.** Three seeds do not support them, and 30
queries over 20 gold entries are not 30 independent observations. Stating this
in the protocol prevents dressing three points up as statistics later.

---

## 5. Scoring — the mapping is frozen HERE, not after

v2's review killed "Regent 4" because the *metrics* were frozen and the
*mapping from metric to score* was not, which left me free to pick the anchor
that suited. The mapping is therefore part of this document.

### 5.1 Equivalence margin, with its denominator named

Two systems are **equivalent** on a metric when

> `|a - b| <= max(absolute_floor, 0.05 * max(a, b))`

and the denominator is `max(a, b)` — **the larger of the two measured values**,
not the theoretical maximum, not the mean. v2 left this ambiguous and the review
called it. Absolute floors, per metric:

| metric | absolute floor |
|---|---|
| `recall_at_budget` | 0.05 (5 recall points) |
| `entries_stored` | 2 entries |
| `mrr` | 0.05 |

### 5.2 Metric → 1–5 anchor

Applied to `recall_at_budget`, per criterion (b), using the **majority of the
five budgets**:

| condition | score |
|---|---|
| wins (outside margin) at ≥4 of 5 budgets | **5** |
| wins at 3 of 5 | **4** |
| equivalent at ≥3 of 5, no majority win either way | **3 — tie, both sides** |
| loses at 3 of 5 | **2** |
| loses at ≥4 of 5 | **1** |

### 5.3 The N/A rule, restated because I broke it in v2

A criterion whose operation a system **does not implement** is **N/A** for that
system — *not* a loss, *not* a 1. In v2 I froze this rule and then scored Hermes
1 on relevance filtering anyway, in Regent's favour, which the review caught.
N/A is recorded as N/A and the criterion is reported as unscorable
cross-system.

### 5.4 Refusal to score

If the arms disagree — one arm says Regent, the other says tie — the criterion
is reported **unscored**, with both arms shown. Disagreement between regimes is
a finding, not a tiebreak to be resolved in either side's favour.

---

## 6. Predictions, recorded before the run

Falsifiable, and deliberately *not* entailed by architecture. v2's five
predictions all "confirmed" and the review correctly noted most were forced by
the metric definitions.

1. In the `raised` arm at B=250, **Regent beats Hermes outside the margin.** A
   small budget is where ranking should pay; block-order delivery gets an
   arbitrary prefix. *Falsified if the gap sits inside the margin.*
2. In the `raised` arm at B=4000, **the two converge to within the margin** —
   a large enough budget delivers most of the store either way. *Falsified if
   Regent still wins outside the margin at the largest budget.*
3. **Hard negatives cost Regent measurably**: its `raised`-arm MRR is lower than
   v2's 0.881, because v2's negatives were lexically easy. *Falsified if MRR
   holds at or above 0.881.*
4. In the `shipped` arm, **both systems store the same count within margin** —
   reproducing v2's capacity tie and confirming the arm is the same regime v2
   measured. *Falsified if they differ outside the margin.*

Prediction 3 is the one most likely to embarrass Regent. It is included for
that reason.

---

## 7. Discharge conditions

- The harness is **audited against both systems**, not only the competitor. v1's
  review caught me discharging "suspect the harness" against Hermes alone.
- No gold entry id may appear in indexed text. Asserted by the builder.
- Both harnesses read the **same** corpus, query and order files.
- If a result is surprising, the **harness is suspected first** — and the check
  is recorded whether or not it changes anything.

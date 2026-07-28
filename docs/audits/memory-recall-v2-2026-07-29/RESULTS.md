# Result — paired memory measurement v2, 2026-07-29

**Outcome: nothing new is scored, v1's one score is superseded by a tie, and the
memory family ends this round effectively unscored.** After two designs and two
adversarial reviews, that is the honest position — and it is a more useful
finding than a scorecard would have been.

Protocol: `../memory-recall-protocol-v2-2026-07-29.md` (frozen `fba3355`).
Pre-run note: `PRE-RUN-NOTE.md` (`19bcbde`). Artifacts and both co-reviews:
`runs/`.

---

## 1. Measurements (3 seeds, mean)

| N | stored: Regent / Hermes | verdict (margin ≤ max(2, 5%)) |
|---|---|---|
| 10 | 10.0 / 10.0 | TIE |
| 20 | 20.0 / 20.0 | TIE |
| 30 | 30.0 / 30.0 | TIE |
| 200 | 32.0 / 30.3 | TIE |

| N | delivered_recall_all | delivered_recall_stored (seeds defined) |
|---|---|---|
| 10 | 0.033 / 0.033 | 1.000 / 1.000 (1 of 3) |
| 20 | 0.033 / 0.033 | 1.000 / 1.000 (1 of 3) |
| 30 | 0.200 / 0.200 | 1.000 / 1.000 (3 of 3) |
| 200 | **0.244 / 0.200** | 1.000 / 1.000 (3 of 3) |

| N | non-gold delivered per query (chars) | |
|---|---|---|
| | Regent | Hermes |
| 10 | 10.0 (690) | 10.0 (690) |
| 20 | 10.0 (682) | 20.0 (1374) |
| 30 | 9.8 (668) | 29.8 (2042) |
| 200 | 9.8 (662) | 30.1 (2064) |

Regent's delivered order is query-dependent at every N (30/30 distinct lists),
MRR 1.000 at N=30 and 0.881 at N=200. **Hermes returns one identical order for
all 30 queries at every N.** Hermes injects 867 / 1583 / 2291 / 2314 chars
unconditionally on every turn.

## 2. Scoring

| Criterion | v2 verdict |
|---|---|
| **d** storage capacity | **TIE** — supersedes v1's "Regent 4" |
| **b** recall | **UNSCORED** |
| **e** relevance filtering | **Regent: measured, works. Hermes: N/A** |
| **c** speed / context cost | **UNSCORED** — one side measured |

### (d) supersedes v1

v1 scored Regent 4 on entries-stored because no equivalence margin existed. v2
predeclared one and applied it blind: every N is a tie. **v1's score is
withdrawn.** The margin's denominator ("5% of what?") was left ambiguous in the
protocol — a v3 defect to fix — but no reading of it changes a tie here.

### (b) is not a tie, and I tried to call it one again

The frozen primary metric gives Regent 0.244 vs Hermes 0.200 at N=200. I
proposed "TIE" on the grounds that the gap traces to capacity rather than
retrieval. **That is the exact v1 error repeated**: using a secondary analysis
to overturn an inconvenient primary result. No recall-specific equivalence
margin was frozen, so none may be applied.

What the numbers support, and no more: equal at N ≤ 30; Regent numerically
higher at N=200; conditional stored recall at ceiling (1.000) for both wherever
it is defined. "Regent's ranking buys zero recall advantage" is **not**
supported — top-10 over ~30 stored entries with sparse gold is too easy for the
comparison to discriminate.

### (e) — Hermes is N/A, and scoring it 1 broke my own rule

I proposed Regent 4 / Hermes 1. Both halves fail:

- **Hermes must be N/A.** My own predeclared rule says a criterion whose
  operation a system does not implement is N/A, *not* a win for the other side.
  Hermes has no search method. Scoring it 1 revokes my rule precisely where it
  protected the competitor. Withdrawn.
- **Regent's 4 was post hoc.** I froze the metrics and never froze the mapping
  from measurement to a 1–5 score. There is no principled reason the numbers
  equal 4 rather than 3 or 5.
- **"Waste" is policy-confounded.** Regent emits ≤10 because *I* set k=10;
  Hermes emits its whole block. The 3× gap is largely encoded by those delivery
  policies before ranking is measured — the v1 denominator artifact under a new
  name. Set k=1 and the "advantage" balloons; set k=30 and it vanishes.
- **"Rank signal" is not a relevance metric.** It detects query-*sensitivity*.
  A hash of (query, doc_id) would score 30/30. My own data prove it: at N=10 and
  N=20 Regent scores 30/30 distinct orders with MRR 0.000, because no gold was
  stored to rank. Demoted to what it actually is — a harness audit showing the
  query reaches Regent's ranking path.

What survives: **Regent ranks and Hermes does not**, which is an architectural
fact plus a working demonstration (MRR 0.881–1.000 where gold exists). The waste
figures are re-labelled **natural-policy context cost** — descriptive, not a
score.

## 3. A reported contradiction that was half right

The reviewer flagged that `delivered_recall_all = 0.033` at N=10/20 is
impossible beside an N/A stored recall. The data were right: seed 22 stored g12
and g14 and answered 3 queries (mean 0.100); seeds 11 and 33 stored no gold at
all (0.000); the mean is 0.033.

But the **aggregation was wrong** — one seed's NaN propagated through
`statistics.mean` and blanked the whole row, which is what made the table read as
self-contradictory. Fixed: the mean now runs over seeds where the metric is
defined, and the count of defined seeds is printed beside it. A real bug, found
by a wrong diagnosis of a real symptom.

## 4. What neither version established

- **Nothing about corpora beyond ~32 stored entries.** Both caps bite at
  2,200 chars, so N=200 is a 200-entry *ingestion* test into a ~30-entry store.
  No evidence on ranking quality, latency or robustness at larger retained
  corpora. To get it, vary the cap — not the offered corpus.
- **Nothing about hard negatives.** v1's distractors were too easy (4.3 shared
  tokens). v2 may have overcorrected to 0.30: lexically dissimilar is not
  semantically hard.
- **No uncertainty.** Means over 3 seeds, no intervals, and 30 queries over 13
  gold entries are not 30 independent observations.
- **The five confirmed predictions prove little.** Most are entailed by
  architecture or by metric definitions — Hermes returns one order because it
  cannot rank; Regent wastes ~10 because k=10. Confirmation of a forced
  prediction is a harness check, not evidence of construct validity.

## 5. Standing after two rounds

Memory family: **0 of 11 criteria cleanly scored.** (d) is a tie, (b) and (c)
unscored, (e) is Regent-measured / Hermes-N/A, and 7 are untouched.

The reason is the finding: **at a corpus size where one system simply injects
everything, retrieval quality and whole-corpus injection are not comparable on
these criteria.** A meaningful comparison needs a regime where Hermes *cannot*
inject everything — which means raising or removing the cap, not enlarging the
offered corpus. That is v3, and it should be designed before it is run.

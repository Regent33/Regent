# Paired memory measurement — protocol v4

**Frozen before the v4 harness or corpus exists.** Git order is the proof.

Supersedes v3 (`7960480`), which its own pre-run review stopped before a single
metric was generated. v1, v2 and v3 all remain in the tree. Three withdrawn
designs are evidence about the measurer, and deleting them would be the
dishonest move.

---

## 0. What killed v3, in one line each

The pre-run review is at
`memory-recall-v3-2026-07-29/reviews/pre-run-review-5.6-sol.md`. Verified, all
of it:

1. **Truth was not retrievable.** The corpus held mutually contradictory claims
   with no timestamp, provenance or correction relation, then expected the
   retriever to surface the one I had labelled true. A false document about
   exactly the requested entity is still highly *relevant*. Sorting truth from
   falsehood there is verification, not retrieval — and had Regent won it, the
   win would have been lexical luck reported as understanding.
2. **`recall_at_budget` was not policy-free.** "Each system truncates in its own
   natural order" *is* the policy, and I chose it.
3. **4 chars/token is not a token budget.** Different text distributions
   tokenize differently, so a shared approximation is not a symmetric error.
4. **The "shipped defaults" arm was not shipped defaults** — the harness passed
   `user_char_limit=2200` where Hermes ships `1375`.
5. **The scorer was not determined by the protocol**: the 1–5 table had
   uncovered cases and the aggregation order was never frozen.
6. **I falsified my own prediction and did not notice.** `entries_stored` is a
   metric under v3 §3.2; the smoke table published 47 vs 44 at seed 11, which is
   outside the frozen margin of 2.35 and disconfirms v3's prediction 4 — under a
   heading claiming no metric had been computed.
7. MRR misses were "undefined" rather than 0 (survivorship bias); the
   equivalence denominator was cosmetic for bounded metrics; text uniqueness was
   unasserted; §2 said 20 queries and §4 said 30.

---

## 1. The estimand, named — not hidden behind "retrieval"

v3 claimed to measure policy-free retrieval. It could not, because one system
has no retrieval. v4 measures the thing that is actually comparable, and calls
it what it is:

> **Gold-fact coverage under an equal rendered-context budget.**
> Regent's query-conditioned selection versus Hermes's query-independent memory
> block, given the same number of tokens of rendered context.

This is a **product-capability comparison**, not an algorithm comparison. It is
deliberately not architecture-neutral: a system that ranks *should* win it, and
the result is only interesting because the budget is equal and the corpus is
identical.

**What v4 does not claim.** It does not measure retrieval quality cross-system,
because Hermes implements no retrieval and is **N/A** on that operation under the
N/A rule (§5.3). Regent's ranking quality is measured separately in §6 against
frozen non-LLM baselines, where the comparison is meaningful.

---

## 2. Corpus — truth must be *in* the data

The v3 corpus was unanswerable. v4 makes the gold decidable from the indexed
text alone, without an LLM and without the annotation.

Every fact carries an explicit **status and date**:

```
CURRENT (2026-06): the billing service runs on postgres 16
SUPERSEDED (2024-03): the billing service ran on postgres 14
REJECTED PROPOSAL (2025-11): the billing service would move to mysql 8
CURRENT (2026-06): the reporting service runs on postgres 15
```

and every query names the **entity, the relation and the currency requirement**:

```
According to the current configuration, which database engine does the
billing service run on?
```

Now a system can win on evidence present in the store — `CURRENT`, the entity
name, the relation — rather than on a label only the scorer can see. A system
matching topic alone still cannot separate these four, because all four share
the topic. That is what makes them hard **and** decidable.

**Structured generation, asserted.** Each fact is generated from a frozen
`(entity, relation, value, status, date)` tuple, and the builder asserts *which
slots match and which differ* per negative — not merely that something was
labelled a negative. v3's assertion counted its own labels and would have passed
`{"topic": "auth", "text": "bananas are yellow"}`.

Per gold, **three** hard negatives, one of each kind:

| kind | entity | relation | value | status |
|---|---|---|---|---|
| superseded value | same | same | **different** | SUPERSEDED |
| rejected proposal | same | same | **different** | REJECTED |
| other entity | **different** | same | different | CURRENT |

- **500 entries**, 20 gold, 60 hard negatives, 420 filler.
- **Global text uniqueness asserted** before anything is written.
- **No gold id in any indexed text**, asserted.
- Gold spread across the corpus, asserted per seed.
- 3 seeds (11/22/33), insertion order emitted once and read by both harnesses.

### 2.1 The ablation corpus

A second corpus, identical in gold, queries, filler, size and insertion order,
with the 60 hard negatives replaced by 60 neutral filler entries **at the same
positions**. This is the only way to attribute anything to hard negatives; v3's
prediction 3 compared a new benchmark against an old one and could not.

---

## 3. Budget — a real tokenizer, over exactly what is rendered

**Tokenizer: `tiktoken` `cl100k_base`.** Named, frozen, and the only one used.
No characters-per-token ratio appears anywhere in v4.

Each harness emits the **exact rendered context** it would hand a model —
Hermes's `format_for_system_prompt` output, Regent's rendered retrieval result —
plus the ordered list of entry ids and each entry's rendered text. **The scorer
does all tokenization and all truncation**, for both systems, in one place. In
v3 Hermes's truncation lived in its harness and Regent's was deferred to a
scorer that did not exist; that alone made the rule unauditable.

Frozen truncation semantics:

- The budget covers **the rendered text including headers and delimiters**,
  because that is what a model is actually charged for.
- Entries are admitted **whole**. An entry that does not fit is skipped and
  truncation **stops** — no packing of later, smaller entries, since neither
  system does that.
- A gold entry counts as delivered only if its **complete rendered text** is
  within the budget.

Budgets: **B ∈ {150, 300, 600, 1200, 2400} tokens**, reported as a curve.

**Primary budget: B\* = 600.** Frozen with its justification: Hermes's shipped
2,200-char cap and Regent's shipped 2,200-char block both render to roughly
550–600 `cl100k_base` tokens, so 600 is the scale at which these products
actually operate. The other four budgets are reported and never used to pick a
winner.

---

## 4. Metrics — frozen

- **`coverage@B`** (primary) = |gold delivered within B| / |gold|, per query.
- **`mrr`** — reciprocal rank of the gold entry in the delivered order. **A miss
  is 0, not undefined.** v3's "undefined" dropped failures and would have
  flattered a system that failed most queries.
- **`entries_stored`** — capacity. Reported per arm and **acknowledged as
  already-observed** for the shipped arm.

Not metrics, stated in advance: rank-signal / distinct-order counts (measures
query-sensitivity, not relevance), latency, and "waste".

### 4.1 Aggregation order — frozen, because it changes the winner

1. per query → `coverage@B`
2. mean over the 20 queries **within** a seed → seed value
3. mean over the 3 seeds → reported value

All 20 per-query outcomes and all 3 per-seed values are printed. The mean over
seeds is never the only number shown.

**Hermes's asymmetry is reported, not hidden.** Hermes makes **one**
context-selection decision per seed and budget, evaluated against 20 target
facts; Regent makes 20 query-conditioned decisions. The macro-average over
queries is therefore *coverage of the 20 target facts by one static prefix* for
Hermes, and that is how it is labelled. No claim treats either system's 20
queries as independent replications.

---

## 5. Scoring — determinate, or refused

### 5.1 Equivalence

`|a - b| <= 0.05` for `coverage@B` and `mrr` (both bounded by 1), `max(2, 5% of
max(a,b))` for `entries_stored`. v3's relative term was cosmetic for bounded
metrics — the floor always dominated — so v4 states the absolute margin plainly
instead of dressing it up.

### 5.2 The mapping — total, with no uncovered case

Scored **at B\* = 600 only**, on the raised arm:

| condition at B* | score |
|---|---|
| A − B > 0.20 | **5** |
| 0.05 < A − B <= 0.20 | **4** |
| \|A − B\| <= 0.05 | **3 — tie, both sides** |
| 0.05 < B − A <= 0.20 | **2** |
| B − A > 0.20 | **1** |

This is a total function on the reals: every possible difference falls in
exactly one row. v3's majority-of-five-budgets rule had gaps (2 wins / 2 losses
/ 1 equivalent had no score) and let a system win on budgets nobody operates at.

### 5.3 N/A

A criterion whose operation a system does not implement is **N/A** for that
system — not a loss. Hermes is N/A on ranking quality (§6). I froze this rule in
v2 and then broke it in Regent's favour; it is restated here for that reason.

### 5.4 Refusal

The shipped and raised arms are **reported separately and scored separately**.
v3 required them to agree, which the review correctly called a way of forcing a
verdict. Where they differ, both are published and the difference is the
finding.

---

## 6. Regent's ranking, measured where it is meaningful

Cross-system ranking comparison is impossible; comparison against frozen
baselines is not. On the same corpus, queries and seeds, Regent's `mrr` and
`coverage@B*` are reported beside:

- **insertion order** (no ranking at all — the floor);
- **BM25**;
- **word TF-IDF**;
- **the same `all-MiniLM-L6-v2` embedding, cosine only** (Regent's vector lane
  without its other lanes);
- **oracle** (gold first — the ceiling).

This answers "does Regent's tri-modal fusion beat the obvious alternatives",
which is a real question with a real answer, and it is where the hard negatives
actually bite.

---

## 7. Harness assertions — required before any artifact is written

From the review, all of them fatal:

- corpus text is globally unique;
- every delivered/ranked id is a known corpus id — the `?nodeid` fallback is a
  **hard error**, never a scored value;
- no id appears twice in a ranking;
- every stored id appears exactly once in a full ranking;
- no refused id is ever delivered;
- when whole-store delivery is claimed, delivered set == stored set;
- the shipped arm constructs **`MemoryStore()` with no arguments** and Regent's
  documented defaults — not a hand-passed pair that merely looks like them.

---

## 8. Predictions

1. At B\* on the raised arm, **Regent's `coverage@600` exceeds Hermes's by more
   than 0.20** (a score of 5). *Falsified by anything less.*
2. **Regent beats BM25 and TF-IDF on `mrr`** on the hard-negative corpus.
   *Falsified if either matches or beats it.*
3. **Regent's `mrr` is lower on the hard-negative corpus than on the ablation
   corpus**, everything else identical. *This is the paired test v3 lacked;
   falsified if the difference is within 0.05.*
4. At B = 2400 on the raised arm, **the two converge to within 0.05**, because a
   large budget delivers most of the store either way.

Prediction 2 is the one most likely to embarrass Regent: if tri-modal fusion
cannot beat BM25 on a corpus built to punish lexical matching, that is worth
knowing and worth publishing.

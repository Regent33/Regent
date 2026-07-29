# Paired memory measurement — protocol v5

**Frozen before the v5 corpus, harness or scorer exists.** Git order is the proof.

Supersedes v4 (`cfa9da0`), killed at design stage by
`memory-recall-v4-2026-07-29/reviews/design-review-5.6-sol.md`. v1–v4 stay in the
tree. Four withdrawn designs are evidence about the measurer.

**v5 changes what is claimed, not just how it is measured.** v4's headline —
"a corpus built to punish lexical matching" — was false, and the correction is
load-bearing: the negatives are **slot-confusable**, and a bag-of-words system
may be unusually strong on them. That is now designed for rather than asserted
away.

---

## 0. What killed v4

Seven defects, all verified against my own frozen text. Three were fatal.

| # | Defect | Repair |
|---|---|---|
| **F1** | `coverage@B` ambiguous: a perfect Regent scores **1.0 or 0.05** depending on reading | §4 — binary `target_delivery@B`, one target per query |
| **F2** | The **raised arm is never defined** — scored on, predicted on, undefined | §3.2 — exact constructor parameter and value, with an assertion |
| **F3** | Score table uses `A − B` with no mapping to products | §6.2 — **A = Regent, B = Hermes**, stated in the table |
| D4 | Harness-side rendering vs scorer-only truncation conflict | §5 — harnesses emit parts; scorer joins, tokenizes, truncates |
| D5 | Baselines had no frozen renderer or admitted population | §7 — one shared renderer, one frozen population |
| D6 | Corpus generator kept post-freeze freedom | §2 — templates, mappings and marginals frozen here |
| D7 | Prediction 4 contradicted the corpus arithmetic | §9 — withdrawn and replaced |

---

## 1. The estimand — narrowed, and the overclaim withdrawn

> **Target-record delivery under an equal memory-context budget.**
> For what share of a frozen query workload does the memory context each product
> actually supplies contain that query's designated target record, given the same
> number of rendered tokens?

A **product-capability** comparison. Deliberately architecture-sensitive:
query-conditioned selection *should* beat a static prefix when the store far
exceeds the context and each query targets a different record.

**What a Regent win would establish:** that its ranking works on explicit,
status-bearing records in this synthetic workload.

**What it would not establish** — and v4 implied — that retrieval memory beats
static memory generally; that Hermes's memory family is inferior; anything about
curated stores, summary-bearing blocks, repeated working sets, answer
correctness after generation, update/consolidation/deletion behaviour, or
utility per unit latency or storage.

**The negatives are `slot-confusable`, not lexically defeating.** In v4's design
the query said "current" and the gold said `CURRENT` while the same-entity
negatives did not — so three lexical signals (entity, relation, currency token)
solve it. That is conjunctive lexical discrimination, not temporal reasoning.
§2.2 adds a stratum where that shortcut is unavailable.

---

## 2. Corpus — two strata, because one of them is a lexical giveaway

Each fact is generated from a frozen tuple
`(entity, relation, value, status, date)`. The builder asserts **which slots
match and which differ** per negative.

### 2.1 Stratum L — lexically aligned (v4's design, kept as the control)

```
CURRENT (2026-06): the billing service runs on postgres 16
SUPERSEDED (2024-03): the billing service ran on postgres 14
REJECTED PROPOSAL (2025-11): the billing service would move to mysql 8
CURRENT (2026-06): the reporting service runs on postgres 15
```
Query: *"According to the current configuration, which database engine does the
billing service run on?"*

The token `current` appears in query and gold. **BM25 is expected to be strong
here, and that is the point of keeping it.**

### 2.2 Stratum D — lexically disjoint, templated

Status is expressed with vocabulary the query never uses:

| role | marker |
|---|---|
| gold | `Status: ACTIVE` |
| superseded | `Status: RETIRED` |
| rejected | `Status: DECLINED` |

Query: *"Which database engine is currently in force for the billing service?"*

No surface token links `currently in force` to `ACTIVE`. A system must carry the
mapping semantically or lose the currency signal. **Three synonym mappings are
frozen below and rotated independently of gold position**, so the mapping cannot
be inferred from position:

| map | gold | superseded | rejected |
|---|---|---|---|
| M1 | `ACTIVE` | `RETIRED` | `DECLINED` |
| M2 | `IN EFFECT` | `WITHDRAWN` | `NOT ADOPTED` |
| M3 | `STANDING` | `LAPSED` | `REFUSED` |

**Query templates are frozen here, four per stratum**, assigned round-robin by
gold index so template is orthogonal to stratum and to insertion position. Every
record reads `{marker} ({date}) … the {entity}'s {relation} is {value}`, so
**entity and relation are shared lexical signals in both strata by design** —
stratum D removes the *currency* signal only.

- **L** (each contains the token `current`, which the gold marker also carries):
  1. *"According to the current configuration, what is the {entity}'s {relation}?"*
  2. *"In the current setup, what is the {entity}'s {relation}?"*
  3. *"Under the current arrangement, what is the {entity}'s {relation}?"*
  4. *"Per the current record, what is the {entity}'s {relation}?"*
- **D** (no token shared with any marker in any map, and never `current`):
  1. *"Presently, what is the {entity}'s {relation}?"*
  2. *"What is the {entity}'s {relation} today?"*
  3. *"As of now, what is the {entity}'s {relation}?"*
  4. *"At this moment, what is the {entity}'s {relation}?"*

> **Pre-build correction, 2026-07-29, before the builder existed.** The first
> frozen D set was *"the standing choice"* and *"the arrangement now in effect"*,
> which collide with the M3 gold marker `STANDING` and the M2 gold marker
> `IN EFFECT`. Two of four D queries would have handed over the exact answer
> token they were designed to withhold. Replaced above, and the builder now
> **asserts** that no D template token appears in any marker of any map, and that
> every L template contains `current`.

### 2.3 Composition

- **500 entries**: 20 gold (**10 stratum L, 10 stratum D**), 60 slot-confusable
  negatives (3 per gold, one of each kind), 420 filler.
- Exactly **one target record per query**, 20 queries, `query_id -> gold_id`
  frozen as a one-to-one map in `targets.json`.
- Global text uniqueness asserted; no gold id in any indexed text; gold spread
  across the corpus, asserted per seed.

### 2.4 Seeds — the unit of analysis

**12 insertion-order seeds** (`101 … 112`), not 3. Hermes's outcome is dominated
by insertion position and this is a non-LLM benchmark where seeds are nearly
free. **The independent experimental unit is the seeded corpus/order, not the
query.** No standard error is computed over 20×12 query outcomes as if they were
independent replications, and query-level variation is never described as
estimating Hermes's policy variance.

### 2.5 The intervention corpus (replaces v4's "ablation")

v4 swapped 60 hard negatives for neutral filler and called the difference the
effect of hard negatives. It is not — that swap also moves document lengths, IDF,
status prevalence, entity/relation frequency and dense-neighbour structure.

v5 builds the counterfactual by **permuting tuple slots**: entity, relation,
value, status and date tokens are recombined across the 60 negatives so that
**unigram marginals are preserved globally** and the query-confusable
*conjunction* is broken. Same ids, same positions, same field count.

Preserved and asserted: per-entry rendered token length (±2), template, status
and date distribution, entity/relation/value token frequency, aggregate
query-term document frequency. **Any marginal that could not be held is
reported in the results**, and the estimand is named the **corpus intervention
effect**, not "the effect of distractor proximity".

---

## 3. Arms — both defined, because v4 defined neither

Every parameter not named here is at the product's shipped default.

### 3.1 `shipped`

- Hermes: `MemoryStore()` — **no arguments** (`memory_char_limit=2200`,
  `user_char_limit=1375`).
- Regent: `GraphMemory::new(store)` with documented defaults, no `with_budgets`
  call.

### 3.2 `raised` — **the primary scored arm**

The memory character budget of each system is set to **200,000 characters**
through that system's own public constructor parameter:

- Hermes: `MemoryStore(memory_char_limit=200_000, user_char_limit=200_000)`
- Regent: `GraphMemory::new(store).with_budgets(200_000, 200_000)`

**Assertion, fatal on failure:** in the raised arm both systems must store
**all 500** submitted entries — `stored == submitted`, `refused == []`, for
every seed and both systems. If either refuses an entry the run aborts; capacity
must be neutralised or delivery is not what is being compared.

Neither system is patched. Both are configured, through the API each publishes.

`shipped` and `raised` are **reported separately and scored separately**. Where
they differ, both are published and the difference is the finding.

---

## 4. Metrics — frozen, and unambiguous

### 4.1 Primary: `target_delivery@B`

For query *q* at budget *B*:

> **1** if *q*'s designated target record (from `targets.json`) is delivered
> **whole** within *B*; **0** otherwise.

Binary. One target. No denominator to misread. v4's `coverage@B` is withdrawn.

Aggregation, frozen:
1. per query → 0 or 1
2. mean over the 20 queries **within** a seed → seed value
3. mean over the 12 seeds → reported value

All per-query outcomes and all per-seed values are printed. The mean over seeds
is never the only number shown. Reported additionally **split by stratum**
(L and D), because they test different things.

### 4.2 Secondary: `mrr`

Reciprocal rank of the target in the **full ranking**, before any budget.
**A miss is 0, not undefined.** Renderer-independent, so it is the honest place
to compare ranking algorithms.

### 4.3 Diagnostics — required, not optional

Per query, per budget: tokens per rendered entry · cumulative tokens by rank ·
target entry token length · number of complete entries admitted · and the
**cause of any miss**, classified as one of

- `rank` — target ranked below the admitted prefix;
- `admission` — target not stored at all;
- `head_of_line` — target ranked inside the prefix but excluded because an
  earlier entry consumed the budget.

Without this a loss cannot be attributed, and v4 had no way to tell these apart.

### 4.4 Not metrics, declared in advance

Distinct-order counts (measures query-sensitivity, not relevance), latency,
"waste", `entries_stored` (already observed; and in the raised arm it is
asserted equal by construction).

---

## 5. Budget and truncation — one place, exactly

**Tokenizer: `tiktoken` `cl100k_base`, version pinned in the results.** No
characters-per-token ratio appears anywhere in v5.

Each harness emits, per query: the ordered list of entry ids it would deliver,
the **rendered text of each entry**, and that system's frozen join template
(`prefix`, `separator`, `suffix`). It performs **no tokenization and no
truncation**.

The scorer, for both systems and all baselines:

1. builds the candidate prefix string by joining the first *k* rendered entries
   with the system's own template, including `prefix` and `suffix` exactly once;
2. tokenizes **the whole joined string** — not per entry summed, because BPE is
   not additive across concatenation boundaries;
3. admits the **maximal complete-entry prefix**: on the first entry whose
   inclusion would exceed *B*, that entry **and all later entries are
   excluded**. (v4 said "skipped and truncation stops", which is contradictory.)

A target counts as delivered only if its **complete rendered text** lies within
the admitted prefix.

**This makes entry length and formatting part of the measured product
capability.** A system can lose by placing a long irrelevant entry early, by
verbose per-entry metadata, or by a correctly-ranked but unusually long target.
That is legitimate for this estimand and it is **not** a clean ranking
comparison — which is why §4.2 and §7 exist.

**Budgets: B ∈ {150, 300, 600, 1200, 2400}**, reported as a curve.

**Primary: B\* = 600** — a **preregistered benchmark point near the nominal
scale of both products' documented block limits**. It is explicitly *not* an
exact translation of a 2,200-character cap: no corpus exists yet, rendered token
count depends on the text, the two products render different headers, and
`cl100k_base` is not necessarily the downstream model's tokenizer. The other
four budgets are reported and never used to pick a winner. This is a
**memory-context budget**, not a claim about what a model is charged.

---

## 6. Scoring

### 6.1 Equivalence

`|a − b| <= 0.05` for `target_delivery@B` and `mrr` (both bounded by 1).

### 6.2 The mapping — total, with A and B named

**A = Regent. B = Hermes.** Scored **at B\* = 600, on the `raised` arm**
(§3.2), on `target_delivery`.

| condition at B\* | score |
|---|---|
| A − B > 0.20 | **5** |
| 0.05 < A − B <= 0.20 | **4** |
| \|A − B\| <= 0.05 | **3 — tie, both sides** |
| 0.05 < B − A <= 0.20 | **2** |
| B − A > 0.20 | **1** |

A total function on the reals: every difference falls in exactly one row.

### 6.3 N/A

A criterion whose operation a system does not implement is **N/A** for that
system — not a loss. Hermes is N/A on ranking quality (§7). Frozen in v2, broken
by me in Regent's favour, restated here for that reason.

---

## 7. Baselines — one renderer, one population, full specification

Ranking quality cross-system is impossible; against frozen baselines it is not.
All baselines rank the **same population** — the 500 stored entries — and are
rendered through **one shared renderer** (`"{text}"`, separator `"\n"`, no
prefix or suffix), so `target_delivery@600` compares ranking and not renderer
verbosity. `mrr` is renderer-independent and reported for all.

| # | baseline | what it isolates |
|---|---|---|
| 1 | random order (seeded) | chance floor |
| 2 | insertion order | no ranking at all |
| 3 | BM25 | lexical |
| 4 | **status-filtered BM25** — filter `status == gold-marker`, then BM25 | the schema-aware method the corpus makes obvious |
| 5 | BM25 + frozen additive status boost (`+1.5` on a gold-marker match) | soft version of 4 |
| 6 | word TF-IDF | lexical, different weighting |
| 7 | char 3–5-gram TF-IDF | morphology / compounds |
| 8 | MiniLM cosine only | Regent's dense lane alone |
| 9 | Regent's FTS lane alone; Regent's graph lane alone | each remaining lane, separately ablated |
| 10 | frozen RRF of BM25 + MiniLM (`k=60`, equal weights) | does fusion beat *naive* fusion |
| 11 | **query-independent static oracle** — the best single fixed prefix under B for the whole 20-query workload | separates "Hermes's policy is weak" from "no static context can work" |
| 12 | **budget-aware query-conditioned oracle** — best achievable given admission and target length, over records actually stored | the real ceiling |

**Frozen implementation details** — "BM25" does not name an algorithm:

- `rank_bm25==0.2.2`, `BM25Okapi`, `k1=1.5`, `b=0.75`
- `scikit-learn==1.8.0` for both TF-IDF variants; word: `sublinear_tf=True`,
  `norm="l2"`, `ngram_range=(1,1)`; char: `analyzer="char_wb"`,
  `ngram_range=(3,5)`
- tokenization for lexical baselines: `re.findall(r"[a-z0-9]+", text.lower())`;
  **no stop-word removal, no stemming**
- embedding: `all-MiniLM-L6-v2`, mean pooling, L2-normalised, cosine
- score normalisation: none before RRF (RRF uses ranks); min-max within a lane
  where scores are combined additively (baseline 5)
- tie-breaking everywhere: **ascending corpus id**, deterministic
- baselines 4 and 5 read the status marker from the rendered text only — no
  access to `targets.json`

**Regent vs its own dense lane is an internal ablation**, and is described as
one: it answers "does fusion add value over its dense component", not "fusion
beats the obvious alternatives". Baselines 3–7 and 10 answer the second.

---

## 8. Harness assertions — before any artifact is written

All fatal:

- corpus text globally unique; `targets.json` is a bijection on 20 queries;
- every delivered/ranked id is a known corpus id — a `?nodeid` fallback is a
  **hard error**, never a scored value;
- no id twice in a ranking; every stored id exactly once in a full ranking;
- no refused id ever delivered;
- **raised arm: `stored == submitted` for both systems, every seed** (§3.2);
- shipped arm constructs `MemoryStore()` with no arguments;
- Regent's vector lane is complete (`embedding_count == stored`) — an incomplete
  lane silently makes it an FTS-only run and understates Regent;
- intervention corpus: the preserved marginals of §2.5 are asserted, and any
  that could not be held are written into the results file.

---

## 9. Predictions

**P1 is a sanity check, not a scientific one, and is labelled as such.** A static
prefix of a 500-entry store at 600 tokens can expose only a small fraction of 20
distributed targets, so a functioning retriever clears +0.20 easily. A large win
here is **not** evidence about ranking sophistication.

1. *(sanity)* At B\*, raised arm, **Regent's `target_delivery` exceeds Hermes's
   by more than 0.20**.
2. *(risky)* **Regent beats status-filtered BM25** (baseline 4) on
   `target_delivery@600` in **stratum L**. Falsified if baseline 4 matches or
   beats it. The review's warning is that BM25 may be unusually strong here.
3. *(risky)* **Regent's margin over BM25 is larger in stratum D than in stratum
   L** — i.e. the advantage survives when the lexical shortcut is removed.
   Falsified if the margin is equal or smaller in D.
4. *(risky)* **Regent beats frozen RRF of BM25 + MiniLM** (baseline 10) on `mrr`.
   Falsified if naive fusion of two public components matches it.
5. *(paired)* **Regent's `mrr` is lower on the confusable corpus than on the
   slot-permuted intervention corpus.** Falsified within 0.05.

v4's prediction 4 — convergence at B = 2400 — is **withdrawn**: at 500 entries a
2,400-token static prefix still reaches well under half the store, so the
arithmetic never supported it.

Predictions 2, 3 and 4 are the ones that can embarrass Regent, and they are the
reason to run this.

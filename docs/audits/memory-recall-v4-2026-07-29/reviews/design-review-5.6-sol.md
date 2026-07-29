








# Verdict

**I would kill v4 in its frozen form.**

Not because explicit status makes the task invalid, and not because the product-capability estimand is inherently dishonest. Those are defensible with narrower claims.

I would kill it because the protocol is still **not executable to a unique measurement**. Several material definitions are missing or contradictory:

1. `coverage@B` is ambiguous in a way that can change a perfect Regent result from **1.0 to 0.05**.
2. The **raised arm is never defined**.
3. `A` and `B` in the scoring table are never mapped to products.
4. Harness-side native rendering/truncation conflicts with scorer-only truncation, especially above 600 tokens.
5. The baseline `coverage@B` comparison has no frozen renderer or admitted-document population.
6. The corpus generator retains major post-freeze degrees of freedom.
7. Prediction 4 is not supported by the stated corpus size and appears incompatible with the native block caps.

Those are design defects, not implementation details. A harness author would have to make consequential choices not determined by this protocol.

---

# 1. Does `CURRENT` make truth retrievable or merely trivial?

## It makes the answer retrievable

The status is evidence in the indexed record. That is not annotation leakage in the v3 sense. If the source record says:

> `CURRENT: billing service runs on postgres 16`

then a retriever is entitled to use `CURRENT`.

The important distinction is:

- v3: truth existed only in an external gold label;
- v4: currency is represented in the source data.

That is a real correction.

## But the present task does not establish semantic handling of currency

Your query contains âcurrentâ and the gold document contains `CURRENT`. The two same-entity hard negatives do not. Meanwhile, the other-current negative lacks the entity. A bag-of-words system can therefore solve the task by adding three lexical signals:

- entity token,
- relation token,
- `current` token.

That is legitimate retrieval, but it means the corpus tests **conjunctive lexical discrimination**, not temporal reasoning or understanding of correction relations.

Consequently, this sentence overclaims:

> âa corpus built to punish lexical matchingâ

The corpus may punish **topic-only** matching. It does not inherently punish lexical matching. Indeed, the exact `current` overlap may make BM25 unusually strong.

## Is that fatal?

**No, provided you narrow the claim.**

Call these âslot-confusable negativesâ rather than negatives designed to defeat lexical retrieval. A Regent win would show that its ranking works on explicit status-bearing records. It would not show that it understands supersession or temporal validity.

## Controls possible without an LLM

I would use at least two frozen strata:

1. **Lexically aligned**
   - query: âcurrentâ
   - record: `CURRENT`

2. **Lexically disjoint but templated**
   - query: âcurrently in forceâ
   - record: `Status: ACTIVE`
   - superseded record: `Status: RETIRED`
   - rejected record: `Status: DECLINED`

The synonym mapping can be manually frozen; no LLM is required. Use several predefined mappings and rotate them independently of the gold positions.

Also add a deterministic, schema-aware baseline:

1. filter `status == CURRENT`;
2. rank remaining records by entity/relation lexical match.

Because your corpus is generated from structured tuples, this is an obvious available method. If Regent cannot beat or approach it, that is important context.

Another useful control is query-template variation: âcurrent,â âactive,â âin force,â and âlatest accepted configuration.â Freeze these before generating results.

Do not remove explicit status entirely. That would recreate v3âs unanswerable task unless you replace it with another decidable validity representation, such as validity intervals and a frozen as-of date.

---

# 2. Is the estimand honest?

## Yes, as a workload-specific product estimand

This is substantially more honest than v3:

> target-fact delivery by the productsâ respective context-selection behavior under a common context-token cap.

It is deliberately architecture-sensitive. Query-conditioned retrieval is supposed to outperform a static prefix when the store is much larger than the context and each query targets a different record. That does not invalidate it.

## No, it is not broad evidence for a âmemory-family criterionâ

The resulting number would support a narrow statement:

> On this synthetic workload, with 500 submitted entries, 20 single-record queries, explicit status markers, and a 600-token memory-context cap, Regent delivered the designated target record more often than Hermes.

It would not by itself establish:

- that retrieval-based memory is generally better than static memory;
- that Hermesâs memory family is inferior;
- performance on curated rather than raw stores;
- performance when a static memory contains summaries rather than atomic entries;
- performance when queries repeatedly target a small working set;
- answer correctness after generation;
- update, consolidation, deletion, or correction behavior;
- utility per latency or storage cost.

The benchmark is structurally favorable to query-conditioned selection. That is permissible for a product-capability test, but it sharply limits external interpretation.

I would also avoid âgold-fact coverageâ unless the metric is precisely about target facts. âTarget-record delivery under an equal memory-context budgetâ is harder to misread.

---

# 3. The primary budget and the curve

Keeping both is the right design:

- **600 is the preregistered decision point**;
- the other budgets expose sensitivity and prevent the primary result from masquerading as budget-invariant.

That is better than selecting a winner from the curve after observing it, and better than reporting only one point.

However, the justification for exactly 600 needs tightening.

## Current problem

You say both 2,200-character blocks render to approximately 550â600 tokens, but:

- no corpus exists;
- rendered token count depends on the actual text;
- the two products may render different headers and delimiters;
- a 2,200-character cap does not imply a fixed token cap;
- `cl100k_base` is not necessarily the tokenizer either productâs downstream model uses.

You have eliminated the token approximation in scoring, but the choice of 600 is still justified using an approximate conversion.

## Acceptable framing

It is fine to freeze 600 as a standardized benchmark budget. State that it is:

> a preregistered benchmark point near the nominal scale of the productsâ documented block limits,

not an exact translation of their shipped character caps.

If you want an empirical calibration, it must use a separately frozen calibration corpus or existing product fixtures, not the eventual evaluation corpus after seeing its behavior.

Also call this a **memory-context budget**, not necessarily the number of tokens âa model is actually charged for,â unless you include all model-visible prompt components and use the downstream modelâs actual tokenizer.

---

# 4. Whole-entry truncation and stop-at-first-nonfit

A maximal-prefix rule is coherent:

> take entries in order until the next complete entry would exceed the cap, then stop.

It does not intrinsically favor one system. But it makes entry length and formatting part of the measured product capability.

A system can lose because it:

- places a long irrelevant entry early;
- has verbose per-entry metadata;
- uses larger delimiters or headers;
- returns a relevant but unusually long target;
- ranks the same relevant entries as a competitor but in a less budget-packable order.

That can be legitimate for the product estimand. It is not a clean ranking comparison.

Two corrections are needed.

## Wording

âSkipped and truncation stopsâ is contradictory. If processing stops, the entry was not âskippedâ in the packing sense. Say:

> The maximal complete-entry prefix is admitted. On the first entry whose inclusion would exceed the budget, that entry and all later entries are excluded.

## Exact accounting

Tokenize each complete rendered prefix, not the individual entries and then sum their token counts. BPE tokenization is not necessarily additive across concatenation boundaries. Headers, separators, and any trailer must be included exactly once.

## Required diagnostics

Report at least:

- tokens per rendered entry;
- cumulative tokens by rank;
- target-entry length;
- number of complete entries admitted;
- whether a miss was caused by rank, admission, or head-of-line length.

For Â§6 ranking baselines, use a **shared renderer**. Otherwise `coverage@600` compares renderer verbosity as well as ranking algorithm. Full-list MRR can remain renderer-independent.

---

# 5. The baseline set

The present set is a useful start but is incomplete.

## What is missing

At minimum:

1. **Random order**
   - Gives a chance floor independent of insertion order.

2. **Status-filtered BM25**
   - Filter to `CURRENT`, then BM25 over entity/relation/text.
   - This is the obvious baseline given the explicit schema.

3. **BM25 plus an explicit status/date boost**
   - Even if implemented as a frozen additive score.

4. **Sparse+dense hybrid**
   - For example, frozen reciprocal-rank fusion of BM25 and MiniLM cosine.
   - Without this, âtri-modal fusion beats obvious alternativesâ is too strong.

5. **Every individual Regent lane**
   - Not just its embedding lane.
   - If Regent has three ranking signals, each should be separately ablated.

6. **A query-independent static oracle**
   - Choose the best single fixed context, under the budget, for maximizing coverage over the frozen 20-query workload.
   - This distinguishes âHermesâs actual static policy is weakâ from âno static context could perform well.â

7. **A budget-aware query-conditioned oracle**
   - Must account for admission and target length, not merely put a target first in a hypothetical population containing records Regent never stored.

Character n-gram TF-IDF would also be reasonable if entity names have punctuation, compounds, or morphology.

## Is Regent versus its embedding lane fair?

Yes. It is a legitimate internal ablation:

> Does the fused method add value beyond its dense component?

It becomes rigged only if described as sufficient evidence that fusion beats the obvious alternatives. It is one component comparison, not a complete baseline suite.

## Missing specifications

Every baseline needs frozen details:

- library and version;
- tokenizer;
- case folding;
- stop words;
- stemming;
- BM25 variant and parameters;
- TF-IDF n-gram range, sublinear term frequency, normalization;
- embedding model revision and pooling;
- score normalization;
- fusion weights;
- tie-breaking;
- candidate population;
- renderer used for budgeted coverage.

âBM25â and âword TF-IDFâ do not uniquely define algorithms.

---

# 6. Is the hard-negative ablation valid?

It is valid for this estimand:

> the total effect of replacing these 60 hard-negative documents with the specified 60 neutral documents.

It is not yet a clean estimate of âthe effect of hard-negative competition.â

Replacing the documents can change:

- document lengths and therefore budget packing;
- query-term document frequencies and BM25 IDF;
- TF-IDF vocabulary and weights;
- status and date prevalence;
- entity/relation frequency;
- dense-neighbor distribution;
- graph or clustering structure used by Regent;
- admission behavior;
- storage capacity if capacity depends on characters or tokens.

Holding count and position fixed does not hold those properties fixed.

## Better counterfactual construction

The replacements should be matched, as closely as possible, on:

- rendered token length;
- template and field count;
- status/date distribution;
- entity-token frequency;
- relation-token frequency;
- value-token frequency;
- aggregate query-term document frequency.

Ideally, construct replacements by permuting tuple slots so unigram marginals are preserved but the query-confusable conjunction is broken. For example, preserve the same entity, relation, status, and value tokens globally while recombining them so the replacement is not a hard negative for that query.

If exact matching is impossible, report the changed marginals. Then call Prediction 3 the **effect of the complete corpus intervention**, not solely the effect of distractor proximity.

You should also define whether replacement entries retain the same IDs. If IDs affect insertion, graph edges, hashes, or tie-breaking, that matters.

---

# 7. Is Prediction 1 forced?

It is not mathematically forced. Regent can fail to store the gold, rank the wrong same-topic entries, or return too few candidates.

But it is structurally expected and therefore has limited diagnostic value.

A static prefix from a large store under a small budget can expose only a small fraction of 20 broadly distributed target records. A functioning query-conditioned retriever only needs to deliver the target for a modest fraction of queries to exceed Hermes by 0.20.

So Prediction 1 is best viewed as a **product sanity prediction**, not a difficult scientific prediction.

That is acceptable if you say so. It becomes misleading if a large victory is interpreted as strong evidence about ranking sophistication or memory architectures generally.

A more informative test would compare Regent with:

- random static selection;
- insertion-order static selection;
- Hermesâs actual static selection;
- the best query-independent static context under the same budget;
- BM25 query-conditioned selection.

That decomposition tells you whether the result comes from:

1. merely being query-conditioned;
2. beating a poor static ordering;
3. beating a strong static selection;
4. ranking better than simple retrieval.

---

# 8. One Hermes selection versus 20 Regent selections

The asymmetry does **not** invalidate a descriptive comparable number.

For a workload consisting of 20 queries, the meaningful question is:

> For what proportion of those queries does the context supplied by the product contain that queryâs target record?

Hermes reuses one context. Regent generates one per query. That is precisely the product behavior being compared.

Reporting the asymmetry is therefore sufficient for the estimand, but not for inference.

You must not:

- treat Hermesâs 20 outcomes as 20 independent context selections;
- calculate standard errors as if there were 60 independent query-level replications;
- claim that query-level variation estimates Hermes policy variance.

The independent experimental unit is closer to the seeded corpus/order, not the query. With only three seeds, any generalization is weak. Three seed values are adequate for a descriptive artifact, but not persuasive evidence about a broad family criterion.

Given that Hermesâs outcome is highly sensitive to insertion position, I would use substantially more insertion-order seeds or exhaustively evaluate many frozen permutations. This is cheap for a synthetic, non-LLM benchmark and directly addresses the dominant source of variation.

---

# 9. Fatal unresolved defects

## 9.1 `coverage@B` is undefined

You write:

> `coverage@B = |gold delivered within B| / |gold|, per query`

There are 20 gold records but apparently one target gold record per query.

Two incompatible implementations are possible:

### Interpretation A: target delivery

For query \(q\):

\[
coverage_q(B) =
\begin{cases}
1 & \text{if q's target is delivered}\\
0 & \text{otherwise}
\end{cases}
\]

A perfect Regent score is 1.0.

### Interpretation B: all-gold coverage

For each query:

\[
coverage_q(B) = \frac{\#\text{ of all 20 gold records delivered}}{20}
\]

A Regent result containing exactly the correct target could score only 0.05.

Your description of Hermesâs static prefix suggests Interpretation A, but the formula suggests Interpretation B. This changes Prediction 1, the score, and the meaning of the estimand. It is fatal.

Freeze the target mapping `query_id -> exactly one gold_id` and define target delivery as a binary indicator. If some queries can have multiple acceptable targets, freeze a per-query relevant set and denominator.

## 9.2 The raised arm is absent

The protocol repeatedly refers to:

- shipped arm;
- raised arm;
- scoring on the raised arm;
- Prediction 1 on the raised arm.

But the full protocol does not define:

- which parameter is raised;
- its value;
- whether it is raised for both products;
- whether storage, rendering, retrieval, or character limits change;
- how the arm interacts with defaults;
- how the arm can render 2,400 tokens.

A harness cannot implement the primary scored arm from this document.

## 9.3 `A` and `B` are unmapped

The scoring table uses `A - B` without saying which product is A and which is B. The score direction therefore is not determined.

## 9.4 Scorer-only truncation conflicts with exact product rendering

The harness emits:

> the exact rendered context it would hand a model

If shipped Hermes internally caps its output around 2,200 characters, the harness cannot emit 1,200- or 2,400-token candidate contexts. The scorer can truncate an output but cannot restore entries the product already omitted.

Similarly, if Regentâs retrieval API returns a fixed top-k or native-size block, the scorer cannot construct larger-budget contexts from missing candidates.

You need to distinguish:

1. the product-native rendered context;
2. the complete ordered candidate sequence;
3. canonical per-entry render fragments from which benchmark prefixes can be constructed.

If you benchmark native outputs, budgets above the native cap should simply plateau or be N/A. If you benchmark counterfactual equal budgets, the harness must request enough ranked candidates to exceed 2,400 tokens, and that is no longer literally the exact context the shipped product would hand to a model.

This distinction likely corresponds to your shipped and raised arms, but those arms are not defined.

## 9.5 Baseline budgeted coverage is underdetermined

For BM25, TF-IDF, embedding, and oracle, what rendered text consumes the budget?

Possible choices include:

- Regentâs renderer;
- Hermesâs renderer;
- a new canonical renderer;
- raw corpus text.

Those can produce different `coverage@600` values. To measure ranking, all baselines need the same canonical renderer and candidate population.

## 9.6 Candidate population and admission are unspecified

If Regent refuses some submitted entries, do baselines rank:

- all 500 submitted records;
- only Regentâs stored records;
- each baselineâs independently admitted records?

Likewise, what does the oracle do when the target was refused?

You need separate quantities:

- submission/admission success;
- ranking conditional on target admission;
- end-to-end target delivery, with refused target counted as a miss.

For ranking ablations of Regent, baselines should normally rank the identical Regent-admitted population. For product capability, refusal remains part of the end-to-end result.

## 9.7 The corpus is not sufficiently frozen

The protocol specifies broad counts and tuple relationships but leaves open:

- entity vocabulary;
- relation vocabulary;
- value vocabulary;
- query templates;
- filler templates;
- neutral filler construction;
- entry lengths;
- gold-position distribution;
- exact meaning of âspreadâ;
- dates and validity consistency;
- accidental lexical overlap;
- number and type of entries each system may refuse;
- tie-producing text patterns.

Because the corpus and builder will be written after the predictions, these are consequential researcher degrees of freedom.

âGold spread across the corpusâ must become a numerical rule, such as fixed strata or exact position sets. The generator algorithm, vocabularies, templates, and random sampling rules should be frozen before results.

Global text uniqueness is necessary but nowhere near sufficient.

## 9.8 Prediction 4 is unsupported

You predict convergence at 2,400 because:

> a large budget delivers most of the store either way.

The corpus has 500 entries. At the demonstrated fact length plus headers and delimiters, 2,400 tokens does not obviously deliver most of 500 entries. It may deliver only a minority.

Further, if native product output is capped around 600 tokens, increasing scorer budget to 2,400 cannot enlarge that output.

Prediction 4 may be falsifiable, but its stated mechanism contradicts the design as written. You need either:

- a precomputed corpus-independent upper bound on rendered store size;
- a budget large enough to contain nearly the whole admitted store;
- or a different prediction.

## 9.9 MRR scope is unclear

You define reciprocal rank in the âdelivered order,â making it effectively `MRR@B`, but Â§6 refers simply to `mrr`.

Freeze whether Â§6 reports:

- MRR over the full ranking;
- MRR after B*=600 truncation;
- both.

Full-ranking MRR is the cleaner ranking metric. Budgeted target delivery is the cleaner context metric.

For Hermes, a reciprocal static context position can be computed descriptively, but it should not be presented as query-conditioned ranking quality.

## 9.10 Versions and tie-breaking are not frozen

The protocol needs exact revisions for:

- Hermes;
- Regent;
- `tiktoken`;
- `cl100k_base` encoding package version;
- `all-MiniLM-L6-v2` model revision;
- BM25 and TF-IDF libraries;
- all retrieval dependencies.

Also freeze deterministic tie-breaking, preferably by insertion position and then stable ID. Otherwise many lexical ties can change coverage at the budget boundary.

---

# 10. Minimum design changes before a harness should exist

I would require these before allowing implementation:

1. Define `query_id -> target_gold_id` and make per-query coverage explicitly binary.
2. Define shipped and raised arms completely, parameter by parameter.
3. Map `A` and `B` to Regent and Hermes.
4. Separate native product output from the ordered candidate stream used for counterfactual budgets.
5. Decide whether budgets above native output are plateaus, N/A, or raised-arm counterfactuals.
6. Freeze a canonical renderer for Â§6 baseline `coverage@600`.
7. Freeze candidate populations and treatment of refused gold for every metric.
8. Specify full-ranking MRR separately from budgeted target delivery.
9. Add status-aware lexical and sparse+dense hybrid baselines.
10. Evaluate every individual Regent lane.
11. Define matched ablation replacements, including token length and document-frequency controls.
12. Freeze corpus vocabularies, templates, generation rules, exact gold-position constraints, and as-of-date semantics.
13. Freeze software/model revisions, parameters, and tie-breaking.
14. Rework or remove Prediction 4.
15. Increase insertion-order replication or explicitly keep all conclusions descriptive and workload-bound.
16. Verify that ordered IDs and rendered entry fragments reconstruct the exact tokenized context byte-for-byte.

---

# Bottom line

The central conceptual repair from v3 is real:

- truth is now represented in the records;
- the cross-product estimand is openly architecture-sensitive;
- tokenizer and aggregation choices are substantially better;
- Hermesâs static-context asymmetry is no longer being disguised.

But v4 is not yet harness-ready. The status cue is **not** what kills it. The honest estimand is **not** what kills it. The fatal issue is that multiple harnesses and scorers, all apparently compliant with the text, could still produce materially different primary results.

Burn the protocol now, principally over the undefined coverage denominator, undefined raised arm, native-output-versus-scorer truncation conflict, unspecified baseline rendering/population, and unfrozen corpus generator.

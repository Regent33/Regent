## Verdict

V2 repairs several obvious defects in v1, but the proposed scoring still does not follow from the frozen design. The central failures are now:

1. **You again override the frozen primary recall metric with a more favorable secondary analysis.**
2. **The relevance-filtering score is confounded by a delivery-budget policy you control.**
3. **You directly violate your predeclared N/A rule when scoring Hermes 1.**
4. **“Rank signal” measures query sensitivity, not relevance. Your own N=10/20 results demonstrate that.**
5. **Nothing here evaluates retrieval over more than roughly 32 stored entries.**
6. **The 1–5 scores and aggregation across N appear post hoc and are not derived from a frozen scoring rule.**

The defensible result is much narrower: **under these particular natural delivery policies, Regent emits less irrelevant context than Hermes at N≥20 and produces query-dependent ordering.** That does not establish a 4–1 relevance-filtering score, a recall tie, or large-corpus scaling.

---

## 1. The proposed recall TIE repeats the core v1 error

Your frozen primary metric is `delivered_recall_all`. Its N=200 result is:

- Regent: 0.244
- Hermes: 0.200

That is not a tie numerically. You did not state that the storage-capacity equivalence margin also applies to recall, and you cannot transfer an equivalence judgment from one metric to another.

Your argument is:

> The recall difference is caused by capacity, not retrieval, so call recall a tie.

That is a causal decomposition introduced after observing the primary result. It is the same structural maneuver as v1: the frozen primary metric gives an inconvenient result, so the secondary analysis is used to determine the verdict.

The predeclared stratification permits you to say:

- Overall delivered recall was higher for Regent at N=200.
- Among target gold that happened to be stored, both systems delivered all of it in the tested conditions.
- The observed overall difference appears attributable to admission/storage rather than failure to deliver stored gold.

It does **not** permit replacing the primary result with “TIE.”

Worse, `delivered_recall_stored = 1.000` is a ceiling result created by the setup:

- Hermes necessarily delivers every stored gold because it emits the whole block.
- Regent returns 10 of only about 30 stored entries.
- Very little gold survives admission.
- The distractors were made lexically easier.
- There apparently are not enough stored relevant targets or hard competitors to make top-10 selective.

Therefore, “ranking buys zero recall advantage” is unsupported. The data show **no observed conditional recall difference under an extremely forgiving top-10 condition**. They do not show equivalence, and certainly not “zero advantage.”

There is also an apparent internal contradiction that must be resolved before scoring anything:

- At N=10 and N=20, `delivered_recall_all = 0.033`.
- Yet `delivered_recall_stored` is N/A because “no query’s gold was stored.”

Nonzero delivered recall is impossible if no query-relevant gold was stored. Rounding can explain waste shown as 10.0 rather than 9.97, but it cannot explain a zero denominator for stored recall. Either the N/A explanation is wrong, the recall calculation is wrong, or “gold” changes meaning between metrics.

### Recall verdict

**Not a legitimate tie.** Report the primary numbers without a categorical tie unless a recall-specific equivalence rule was frozen. The correct interpretation is “equal at N≤30, Regent numerically higher at N=200; conditional stored recall reached a ceiling for both where defined.”

---

## 2. Waste is substantially a k=10 versus whole-block artifact

Yes. You have renamed the denominator artifact rather than eliminated it.

At N=200:

- Regent is permitted to emit at most 10 entries.
- Hermes emits about 30 entries.
- You then count how many irrelevant entries each emitted.

Given sparse relevance, Regent is nearly guaranteed to have about 10 non-gold deliveries and Hermes about 30. The roughly 3× waste ratio is largely encoded by the output-size policies before ranking quality is measured.

This does not make the result meaningless. It makes it a measurement of a different thing:

> **Default end-to-end context footprint under each system’s natural delivery policy.**

That is a valid product-level comparison. It is not an isolated measurement of relevance filtering.

Because you control Regent’s k, the result is trivially manipulable:

- Set k=1 and Regent’s waste collapses.
- Set k=30 and most of the apparent advantage disappears.
- Set k=0 and waste becomes perfect while recall becomes zero.

A metric that can be improved merely by shrinking the output must couple waste to recall or hold the output budget constant.

Chars do not solve this. Entry count and character count are both determined heavily by how much each system is allowed to emit. They are two representations of the same policy confound.

### An unrigged comparison

You need at least two separate evaluations:

#### A. Natural-policy product comparison

Keep Regent top-10 and Hermes whole block, but label the result correctly:

- delivered recall;
- total delivered tokens/chars;
- irrelevant delivered tokens/chars;
- latency if measured comparably.

This answers: “What does each product inject under its default/natural behavior?”

It does not isolate ranking quality.

#### B. Controlled relevance-efficiency comparison

Use one or more of:

- **Recall at matched token budgets**
- **Waste at matched recall**
- **Precision at matched delivery counts**
- **Recall-versus-context-cost curves**
- **Area under a recall/cost frontier**
- A frozen sweep over k or token budgets, such as 1/3/5/10/20/30

For Hermes, a fixed insertion-order prefix can be evaluated as a no-ranking baseline at each budget. If truncation is not an operation Hermes supports, then that controlled comparison is diagnostic rather than a cross-system product score. Under your N/A rule, ranking itself remains N/A for Hermes.

You also need enough relevant and hard-negative material in stored memory for changing k to create a meaningful tradeoff. Right now top-10 over approximately 30 entries with very sparse stored gold is too easy.

---

## 3. “Rank signal” is not a relevance metric

“Distinct delivered orders across queries” is not rank quality. It measures only whether the output permutation depends on the query.

A system can score 30/30 by sorting documents according to a hash of:

```text
hash(query || document_id)
```

That would be perfectly query-dependent and completely unrelated to relevance.

Your own data refute the proposed interpretation:

- At N=10 and N=20, Regent has 30/30 distinct orders.
- Its MRR is 0.000.
- You state that no gold was available to rank.

Thus Regent exhibits maximum “rank signal” in conditions containing no demonstrated relevance signal whatsoever. The metric detects query-conditioned motion, not useful ranking.

Use it as a **harness audit**:

> “The query reaches Regent’s ranking path, and results are not invariant.”

Do not use it as evidence for relevance filtering.

MRR is the actual relevance metric here, but even that needs care:

- condition it on target availability when analyzing ranking;
- keep overall MRR separately for end-to-end performance;
- compare over a common candidate set when trying to isolate ranking;
- report uncertainty;
- include harder negatives and multiple relevant items;
- preferably add nDCG, MAP, precision@k, or recall-cost curves.

Hermes returning one order is unsurprising because it does not rank. “One distinct order” is an architectural fact, not an empirical quality finding.

---

## 4. Hermes must be N/A under your frozen rule

Your rule says:

> A criterion whose operation a system does not implement is N/A for cross-system scoring, not a win for the other side.

You then say:

> Hermes has no search method at all, so it scores 1 for relevance filtering.

That is a direct violation.

You cannot have both:

- “No implementation means N/A,” and
- “No implementation means score 1.”

If criterion (e) is specifically **search/ranking/filtering functionality**, Hermes is N/A.

If criterion (e) is instead an **end-to-end outcome such as irrelevant context delivered under natural operation**, then both systems can be evaluated—but the result must be called context efficiency or delivery waste, not search quality. It remains policy-confounded and cannot be converted into “Regent 4, Hermes 1” without a frozen score mapping.

You may conclude that the N/A rule was a bad design choice because it prevents absent functionality from being penalized. But it was frozen. You cannot revoke it only where it protects Hermes.

### Therefore

**Regent 4 / Hermes 1 is not defensible under the stated rules.**

At most:

- relevance ranking: Regent scored; Hermes N/A;
- natural-policy delivery waste: both reported descriptively;
- query-dependence audit: Regent passes; Hermes does not implement ranking.

---

## 5. The 4 and 1 scores are themselves post hoc

Where is the frozen transformation from measurements to a five-point score?

Why does:

- 30/30 query-dependent orders,
- about 10 irrelevant deliveries,
- perfect conditional recall in two conditions,
- and no filtering at N=10

equal exactly **4** rather than 3 or 5?

Why should N=10 deduct one point? Why is each N weighted equally? Why does failure to filter when `k >= corpus size` count as a system deficiency rather than a degenerate test condition? Why does Hermes’s absent operation map to 1 despite the explicit N/A rule?

Unless the quantitative thresholds and cross-N aggregation were frozen, the numerical grades are narrative judgments applied after seeing the table. Freezing metrics does not freeze scoring.

Report the measurements. Do not pretend the 4–1 mapping is objective.

---

## 6. The capacity “ties” are also weaker than claimed

Predeclaring a margin fixes the v1 retroactivity problem, but several defects remain.

### The 5% reference is unspecified

Five percent of what?

- requested N;
- Regent count;
- Hermes count;
- their mean;
- the smaller count;
- the larger count?

At N=200, 5% of requested N is 10 entries, while 5% of approximately 31 stored entries is about 1.55. Those are radically different margins. “Max(2, 5%)” is not operationally complete without the denominator.

### Means within a margin do not establish equivalence

With three seeds, showing that the difference between means is 1.7 does not by itself establish equivalence. A proper equivalence claim requires uncertainty around the paired difference and a confidence interval inside the equivalence bounds. At minimum, show every seed and apply the rule per seed or provide a paired interval.

### Entry count is an unstable capacity unit

The limit is character-based. Capacity should primarily be measured in:

- retained payload characters or tokens;
- retained semantic records;
- serialization and metadata overhead;
- and perhaps entries as a secondary result.

“32 versus 30.3 entries” can reflect small differences in record length or overhead rather than meaningful storage capacity.

---

## 7. There is still no evidence about retrieval scaling beyond ~32 stored entries

N=200 is not a 200-entry retrieval experiment. It is a 200-entry **ingestion attempt into a ~30-entry store**.

The ranking system never retrieves over 200 stored candidates. It retrieves over approximately 32. Therefore:

- no evidence supports ranking quality at 60, 100, or 200 stored entries;
- no evidence supports latency scaling;
- no evidence supports index quality under larger retained corpora;
- no evidence supports context efficiency when both systems can retain larger corpora;
- no evidence supports robustness as hard-negative density grows.

N=30 and N=200 have nearly the same retained-set size. Their main difference is which records survive admission, not search-scale.

To establish scaling, vary the cap so that actual stored counts reach, for example:

- 10, 30, 60, 100, 200, 500.

Then separately vary offered corpus size above capacity to measure admission/eviction behavior. Storage saturation and retrieval scaling are different experiments and should not be collapsed into N.

Your redistribution of gold fixes the v1 “cap protects gold” problem, but it introduces another problem: very few gold items survive, making conditional top-10 recall easy and noisy. You need controlled gold prevalence among retained candidates, not merely gold scattered beyond the storage boundary.

---

## 8. The benchmark appears too easy for the ranking claim

The distractor rebuild may have overcorrected.

Reducing shared content tokens from 4.30 to 0.30 does not necessarily make distractors better. It may make them obviously irrelevant. Lexical dissimilarity is not evidence of semantic hardness.

A serious retrieval test needs:

- semantically plausible hard negatives;
- lexical confounders;
- paraphrastic near-misses;
- entities and facts with overlapping attributes;
- multiple relevant documents;
- relevance judgments independent of the retrieval implementation.

MRR near 1.0 under sparse gold and weak lexical overlap may show that Regent works, but it may also show that the task is trivial.

---

## 9. Three seeds and means are insufficient for the claims being made

You report means but not:

- per-seed values;
- variance;
- paired differences;
- query-level distributions;
- confidence intervals;
- effective sample size given paraphrases or repeated gold targets.

Thirty queries are not necessarily 30 independent observations. If several queries target the same gold item or are paraphrases, the effective sample size is lower.

This particularly matters for:

- the 0.244 versus 0.200 recall difference;
- the 32.0 versus 30.3 capacity comparison;
- MRR 0.881;
- equivalence claims.

“Three seeds” is not a substitute for uncertainty analysis.

---

## 10. Five confirmed predictions are not evidence that the design was sound

Not without seeing the exact predictions and their failure regions.

Several likely predictions are almost logically forced by the setup:

- Hermes emits one order because it has no query-dependent ranking.
- Hermes has perfect stored recall because it emits everything stored.
- Regent emits at most 10 entries because k=10.
- Regent therefore wastes roughly 10 entries when relevance is sparse.
- Capacity is called a tie because the predeclared margin is wider than the expected small difference.

Those are implementation checks, not risky scientific predictions.

Confirmation is probative only when predictions are:

- quantitatively specific;
- capable of failing by meaningful margins;
- not entailed by architecture or metric definitions;
- made without testing the same corpus/seeds;
- evaluated on held-out data;
- accompanied by explicit falsification criteria.

Also, git commit order does not by itself establish that no exploratory run occurred before commitment. If the corpus, seeds, thresholds, or predictions were tuned using pilot runs on the same benchmark, the predictions are not out-of-sample.

“All five confirmed” may indicate harness consistency. It does not validate construct validity.

---

## Direct answers

- **Is Regent 4 / Hermes 1 defensible?**  
  No. Waste is heavily determined by top-10 versus whole-block delivery, rank-signal is not relevance, the 1–5 mapping is not justified, and Hermes must be N/A under the frozen rule.

- **Does control of k rig the waste comparison?**  
  Yes, if waste is interpreted as ranking quality. It is acceptable only as a descriptive natural-policy context-cost result. Use matched budgets, matched recall, and recall-cost curves for an unconfounded comparison.

- **Is rank signal real or tautological?**  
  It is a weak query-path audit, not a relevance metric. A random query-conditioned permutation would pass. The N=10/20 results prove this.

- **Should Hermes be 1 or N/A?**  
  N/A under the exact rule you froze. Scoring 1 violates that rule.

- **Is recall now legitimately a tie?**  
  No. The primary metric favors Regent at N=200, and no recall-specific equivalence rule was stated. The stratification explains the difference but does not erase it.

- **Does this support conclusions above ~32 entries?**  
  No. It tests admission under a larger offered corpus, not retrieval over a larger stored corpus.

- **Do five confirmed predictions validate the design?**  
  No. They may be architecture-entailing or weak predictions. Without quantitative falsification regions and held-out evaluation, they are mostly harness sanity checks.

## Defensible bottom line

V2 supports only this:

> Under the tested fixed cap and natural delivery policies, Regent delivered less irrelevant context than Hermes at N≥20 and produced query-dependent rankings. Both systems delivered all query-relevant gold that survived storage in the conditions where the stored-recall metric was defined, although the test appears ceiling-limited. No result establishes retrieval scaling beyond approximately 32 stored entries.

Anything stronger—especially **Regent 4 / Hermes 1**, **recall TIE**, or claims about larger corpora—is not supported by the frozen design.
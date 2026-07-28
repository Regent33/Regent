# Adversarial co-review — GPT 5.6 sol (read-only, no tools)

Sent after the runs completed and before any score was committed.
Prompt: co-review-prompt.txt

---

Your proposed reading is not defensible as written. It correctly identifies that Hermes is not a retrieval system, but then treats incompatible measurements as if they supported clean ties and wins. They do not.

## 1. Regent’s 0.103 precision is almost entirely arithmetic

The reported values expose the problem:

- Regent: approximately \(31 / (30 \times 10) = 0.103\)
- Hermes at 20 entries: approximately \(31 / (30 \times 20) = 0.052\)
- Hermes at 30 entries: approximately \(31 / (30 \times 30) = 0.034\)

Apparently there are 31 total gold memberships across the 30 queries. Once recall is 1.0:

- Regent’s precision is forced to 0.103 because it always emits ten items.
- Hermes’s “full block precision” is forced to 0.052 or 0.034 because it emits 20 or 30 items.

Thus these figures largely measure output-set size, not ranking quality. Calling 0.103 an “arithmetic ceiling” is essentially right: given the gold cardinalities and an exactly ten-item result, Regent cannot exceed it. Regent reaching that ceiling proves that all gold items are in its top ten, which is useful, but the precision number itself adds almost no information beyond recall@10.

The claimed criterion-(e) conclusion therefore does **not** survive as a standard retrieval-precision conclusion. At most, it supports:

> Regent provides a denser context block under the tested output policies: roughly one relevant item per ten supplied entries, versus roughly one per 20–30 for Hermes.

That is context density or context efficiency, not a clean comparison of retrieval precision.

There are two possible comparisons, neither matching your conclusion exactly:

- **Equal cutoff, precision@10:** Regent is 0.103 and Hermes is 0.077. But Hermes’s first ten are file order, so this compares retrieval ranking against arbitrary storage order.
- **Actual model context:** Regent is denser, but the systems are allowed different output sizes and different invocation policies. This is a system-level context-cost comparison, not precision@k.

Anchor 5/2 is therefore unjustified unless “precision” was explicitly frozen as relevance density over each system’s actual delivered context. The frozen protocol instead says precision@5 and precision@10.

## 2. Calling recall a tie improperly changes the frozen metric

The frozen metric was recall@k for \(k \in \{5,10\}\). Under that metric, the observed recall@10 result is:

- Regent: 1.000
- Hermes: 0.750

Numerically, Regent wins. You cannot retroactively replace recall@10 with “recall over everything reaching the model” merely because the original metric behaves badly for Hermes.

The diagnostic establishes that Hermes’s recall@10 is not query-dependent retrieval quality. It is file-position luck. That means the benchmark is ill-specified for comparing these architectures—not that the frozen recall@10 result becomes a tie.

There are three distinct criteria:

1. **Retrieval recall@10:** Regent wins; Hermes has no retrieval ranking, so the comparison is conceptually malformed.
2. **Information availability in supplied context:** Both achieve 1.0 on this small, capped corpus.
3. **Scalable memory recall under a context budget:** Not measured.

Calling criterion (b) a tie conflates the second with the first. If (b) is genuinely a retrieval criterion, the more defensible result is **N/A or UNSCORED for cross-system comparison**, not a tie. Hermes does not retrieve. Its full-block recall is tautological: if every stored item is injected, every stored gold item is available.

It is still meaningful to measure that tautology as system behavior—the model does receive the information—but it is not evidence that Hermes has good retrieval.

Also, invoking the “stability rule” does not solve this. No uncertainty interval is provided, and the problem is metric applicability, not an interval straddling an anchor boundary.

## 3. The speed result is not a valid outright Hermes win

“0.000 ms” is not a credible latency estimate. It means “below the measurement resolution or rounded to three decimals,” not zero. At minimum, it should be reported as an upper bound or with a higher-resolution timer and a latency distribution.

More importantly, the timed operations are not equivalent:

- Regent performs query embedding, ranking, and possibly graph expansion.
- Hermes returns a cached pointer/string whose query-independent rendering occurred outside the timed section.

If criterion (c) is narrowly defined as:

> Time spent by the memory backend after the memory text or query is already available,

then Hermes is faster. But that is an unsurprising microbenchmark of “cached string access versus actual retrieval,” not equivalent memory functionality.

If the intended criterion is end-to-end memory overhead per turn, the measurement needs to include:

- Hermes prompt-token processing caused by injecting the block every turn.
- Any formatting or snapshot invalidation costs.
- Regent tool-selection/dispatch overhead.
- Regent retrieval computation.
- The model’s processing of the returned context.
- Turns where Regent’s memory tool is not called.

On that criterion, the current data say nothing decisive. Hermes’s repeated prompt cost may dominate its dictionary lookup, while Regent pays retrieval cost only on selected turns.

Therefore:

- The raw backend timing favors Hermes.
- A functional or end-to-end speed criterion is **UNSCORED**.
- Anchor 5 Hermes / 2 Regent is unsupported.

The source audit’s claim of a tie may also be unsupported, but replacing it with an outright Hermes win does not repair the comparison.

## 4. The capacity result is not a tie under the frozen metric

The frozen metric is “entries stored.” The observed counts are:

- N=20: tie, 20–20
- N=60: Regent wins, 31–30
- N=200: Regent wins, 31–30

Under the stated anchors—“wins at at least two of three N”—that is nominally a Regent win, likely anchor 4, not a tie. A one-entry difference may be practically negligible, but no equivalence margin was frozen. You cannot introduce one now.

The byte-identical character caps support a different conclusion:

> The systems have essentially the same nominal character-budget capacity, with a one-entry difference arising from formatting, accounting, or the exact entry-length sequence.

But that is not the frozen “entries stored” metric.

Consequently, “my prediction that Regent would win is disconfirmed” is false on the literal evidence. Regent stored more entries at two reported N values. If the prediction meant a substantial capacity advantage, that stronger prediction was not supported—but it was not cleanly disconfirmed without a predeclared practical-equivalence threshold.

## 5. The 2,200-character cap destroys the claimed scaling evidence

This is the largest flaw.

The labels N=60 and N=200 do not describe the effective indexed corpus:

- Regent effectively tests 31 stored entries at both N=60 and N=200.
- Hermes effectively tests 30 stored entries at both N=60 and N=200.

Therefore N=200 is not a 200-entry retrieval experiment. It is a failed-write experiment followed by retrieval over approximately 30 entries.

Claims such as:

- “Regent stays flat as N grows”
- “Hermes degrades as N grows”
- “Regent degrades gracefully”
- “wins at every N”

are misleading. The stored corpus stops growing. N=60 and N=200 are effectively duplicates for retrieval purposes.

Hermes’s precision falls from 0.052 to 0.034 only because its delivered block grows from 20 to 30 entries. It does not degrade further at nominal N=200 because it stores nothing further. Regent’s flatness is also partly structural: it always emits ten items, and its effective corpus never exceeds 31 entries.

The cap does not make **all** results uninformative. The experiment still tells you:

- The write path stores only about 30 short entries.
- Hermes exposes all stored entries query-independently.
- Regent can place the tested gold items in its top ten within a corpus of at most 31 stored entries.
- Hermes has higher mandatory context volume under this setup.
- Regent’s retrieval backend takes roughly 4.6 ms in this tiny regime.

It tells you almost nothing about retrieval behavior at realistic corpus sizes, distractor robustness beyond approximately 30 entries, or latency scaling.

## 6. Putting all gold entries first fatally weakens the distractor test

All 15 gold entries are in the first 20, while later entries are distractors. Once the stores hit capacity:

- Gold writes have already succeeded.
- Later distractor writes are refused.
- Increasing nominal N preferentially removes distractors rather than relevant items.

That means the capacity cap protects recall. The frozen rule that refused gold writes count as misses never matters here, because corpus order ensures the gold is accepted before saturation.

This is a serious design bias. A capacity-limited append-only store should be tested with:

- randomized insertion order,
- gold positions distributed throughout the corpus,
- multiple order seeds,
- or explicit replacement/eviction behavior.

Otherwise “robustness as N grows” is not tested. The systems simply stop admitting the additional hard cases.

## 7. Prediction 1 required a Regent harness audit, not only a Hermes diagnostic

The frozen prediction explicitly said:

> If Regent also wins at N=20, suspect the harness before believing it.

Regent did win at N=20 under recall@10. The response was to diagnose Hermes’s file order. That explains Hermes’s low recall@10, but it does not audit Regent’s perfect recall.

You still need to verify, at minimum:

- Regent’s returned IDs for every query.
- Whether results vary appropriately by query.
- Whether query text leaks gold identifiers or near-exact lexical material.
- Whether graph expansion indiscriminately brings many gold entries into the top ten.
- Whether gold annotations enter indexing or scoring accidentally.
- Whether duplicate entries or ID-resolution bugs inflate recall.
- Whether ranking survives randomized corpus order and randomized gold positions.
- Recall@5, not only recall@10.
- Relevant-item ranks, MRR, or nDCG—not merely whether the item appears somewhere in ten results.

Until that audit is done, the frozen suspicion condition remains unresolved.

## 8. The required recall@5 and precision@5 results are missing

The protocol froze both \(k=5\) and \(k=10\), but only k=10 is presented. You cannot score the frozen retrieval criteria from half the required measurements.

This matters because Regent could have:

- recall@10 = 1.0,
- but substantially worse recall@5,

which would reveal weak ranking despite successful broad candidate generation. Conversely, Hermes’s arbitrary file order could behave differently at five.

Any final anchor assignment before reporting both cutoffs is premature.

## 9. The three N conditions are not independent evidence

At N=60 and N=200, each system stores the same number of entries and apparently the same prefix. Those are not two meaningful scale conditions. Counting them as separate wins to satisfy “wins at two of three N” double-counts essentially the same effective corpus.

The anchor scheme assumes N corresponds to actual increasing retrieval difficulty. Here it does not.

## Bottom line

The defensible conclusions are narrower than yours:

- **Recall:** Regent wins the frozen recall@10 calculation, but the cross-system retrieval interpretation is malformed because Hermes has no ranking. Full-context availability recall is tied at 1.0 only for the tiny accepted corpus. Criterion-level scoring should likely be UNSCORED, not declared a tie.
- **Precision:** The reported full-context values are mostly denominator arithmetic. Regent has better context density, but anchor 5/2 as “retrieval precision” is not justified.
- **Capacity:** Under the literal frozen metric, Regent wins 31–30 at two nominal N values. Calling it a tie requires an equivalence margin that was not predeclared.
- **Speed:** Hermes wins a narrow cached-string-access microbenchmark. Functional/end-to-end memory latency was not measured, so anchor scoring is unjustified.
- **Scaling:** Not measured. Effective corpus size never exceeds 31, N=60 and N=200 are duplicates, and gold-first insertion filters out later distractors.
- **Overall audit verdict:** You have not established that the source audit is wrong in three of four rows. You have established that several rows are based on criteria that are either incompatible across the two architectures or inadequately measured.
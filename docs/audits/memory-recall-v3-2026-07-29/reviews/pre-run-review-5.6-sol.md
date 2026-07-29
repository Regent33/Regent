







# Verdict

**Do not run the scorer. v3 is still not a valid frozen measurement.**

You removed the storage-cap confound from the raised arm, but replaced it with three larger problems:

1. **The purported token budget is neither token-based nor based on either systemâs actual delivered context.**
2. **The corpus does not make âtruthâ retrievable. It asks Regent to distinguish mutually contradictory documents when the query contains no information that distinguishes them.**
3. **The scorer is not frozen because it does not exist, and the protocol leaves decisive aggregation and truncation choices unspecified.**

There are also direct contradictions in the frozen protocol and harness:

- You say no metric has been computed, but `entries_stored` is explicitly a metric and you have already observed it.
- Prediction 4 has already taken a hit: at seed 11, 47 versus 44 is **outside your own margin**.
- You call the Hermes shipped arm âshipped defaults,â but the harness changes `user_char_limit` from 1375 to 2200.
- You say truncation lives in one scorer, but Hermes truncation is implemented inside its harness while Regent truncation is deferred to the nonexistent scorer.
- The protocol says 20 queries, then refers to â30 queries over 20 gold entries.â
- You call the task retrieval/relevance filtering while also freezing an N/A rule under which Hermes, which has no retrieval, should be N/A.

The raised arm may eventually support a useful **context-selection comparison**, but it does not currently support the claims in this protocol.

---

# 1. `recall_at_budget` is not policy-free

No. âIts own natural orderâ is a policy choice. You have not eliminated policy; you have made ordering policy the entire treatment.

That is not automatically illegitimate. A fixed-budget end-to-end question can reasonably be:

> Given the systemâs own context-selection policy, how many required facts enter a common context budget?

But that is **context allocation**, not pure retrieval quality and not policy-free.

More seriously, neither arm currently uses an actual natural delivery under the imposed budgets:

- Hermes naturally emits the whole block. It does not naturally emit your manually constructed entry-prefix at B.
- Regent is asked for `k = 500`, which may not be its normal delivery policy. You then plan to impose an external prefix.
- You have not established how either system serializes those entries into the actual model prompt.
- You have not established what real prompt truncation does when the full context does not fit. It may truncate from the tail, truncate from the head, drop a section, summarize, or reject the prompt.

Your claim that the test measures âwhat each system puts in B tokensâ is therefore false. It measures what your harness **imagines** each system would put in B under a synthetic whole-entry prefix rule.

## The unrigged versions

You have to choose the estimand explicitly.

### Option A: End-to-end context selection

Measure the exact serialized context each product supplies to the model under a common external context allowance:

- call the normal product delivery path;
- capture the exact rendered context, including headers, delimiters, metadata and formatting;
- apply the real downstream tokenizer;
- use the actual truncation/context-building behavior;
- score gold entries only if their complete identifying content appears in that actual context.

This legitimately rewards Regent for ranking and leaves Hermes as a static-injection baseline. Call it something like:

> gold-fact coverage under delivered-context budget

Do not call it policy-free retrieval.

### Option B: Ranking-only evaluation

If the question is retrieval quality, Hermes does not implement the operation and is N/A under your own rule. Evaluate Regent against frozen baselines instead:

- random/insertion order;
- BM25;
- TF-IDF;
- embedding-only ranking;
- perhaps an oracle ceiling.

Do not invent a Hermes ârankingâ from its storage order and then say both systems implement retrieval.

### Option C: Static versus query-conditioned context allocation

This is probably your actual intended comparison. State it plainly:

> Regentâs query-conditioned ranking versus Hermesâs query-independent memory block under equal rendered-context budgets.

That is fair as a product-capability comparison. It is not architecture-neutral and should not be.

---

# 2. The four-characters-per-token approximation is unacceptable

Yes, it can bias the systems differently.

Applying the same approximation does not make the error symmetric. Regent and Hermes select different text distributions at small budgets. Technical strings have highly variable tokenization:

- `eu-west-2`
- `us-east-1`
- `feat/`
- `pytest`
- `xdist`
- `postgres 16`
- `09:00`
- `apache 2.0`
- punctuation and delimiters

If Regent preferentially ranks technical entries while Hermesâs prefix contains more filler, the two delivered sets can have materially different characters-per-token ratios. The approximation can therefore change which entries cross the boundary and can change recall.

You also are not even approximating the same serialized object:

```python
cost = len(text_of.get(cid, "")) + len("\nÃÂ§\n")
```

This is a guessed per-entry cost, not demonstrated to equal `format_for_system_prompt()` output. Your smoke result already shows that the formatted Hermes block can exceed the nominal cap:

- cap 2200
- formatted block 2347 characters

That is evidence that storage accounting and delivered-context accounting differ.

Regentâs actual rendering overhead is completely absent. Scores, metadata, graph information, bullets, headers and separators may consume context. Counting only `h.node.content` is not a delivered-context budget.

## Required correction

Use one frozen tokenizer over each systemâs **exact rendered context**. Pick either:

- the tokenizer of the downstream model being targeted; or
- an explicitly named neutral tokenizer, if this is intentionally model-independent.

Then freeze:

- exact serialization;
- special-token handling;
- whether the budget includes headers and separators;
- whether partial entries are permitted;
- whether an entry counts as delivered if only part of it survives;
- what happens when the next entry does not fit.

Do not use four characters per token.

If you refuse to select a tokenizer, use an exact byte or Unicode-scalar budget and call it that. Do not label a character estimate as tokens.

---

# 3. `k = whole corpus` is defensible only for a ranking benchmark

Asking Regent for the whole corpus and truncating later is a normal way to obtain a full ranking. It does not unfairly advantage Regent within a ranking evaluation; ranking is the feature being evaluated.

But it creates two problems here.

## First: it is not necessarily natural product behavior

If Regent normally retrieves a smaller default `k`, then `k=500` is a synthetic evaluation configuration. That may be appropriate, but stop calling the resulting list its natural delivery.

You need separate wording:

- **full ranking generated by Regent**, then
- **evaluation prefix chosen under the common budget**.

## Second: `k=500` does not prove you received a complete ranking

The retrieval implementation may have internal candidate limits, lane-specific cutoffs, deduplication or filtering. You assert that all stored entries have embeddings, but not that all stored entries appear exactly once in each ranked list.

You need assertions for every query:

- ranked length equals the number stored, unless the API explicitly documents otherwise;
- every returned ID belongs to the stored set;
- no ID appears twice;
- every stored ID appears exactly once;
- no `?unknown` fallback occurs;
- no refused ID appears.

Without those, âwhole corpus rankingâ is merely an argument passed to the API.

## The supposed disadvantage to Hermes

Hermes having no ranking is not a harness defect. If you are testing fixed-budget context usefulness, absence of ranking is exactly the product limitation.

It becomes rigged only when you describe the result as a comparison of two retrieval algorithms. One side has no retrieval algorithm.

---

# 4. The shipped arm is resource-confounded

The two caps are not demonstrably the same unit.

- Regent counts `chars().count()` over entry text.
- Hermes appears to account over a delimiter-joined representation.
- Hermesâs formatted prompt has additional overhead.
- Python `len()` and Rust `.chars().count()` are broadly comparable for Unicode scalar/code-point counting, and this corpus is mostly ASCII, but the larger issue is **what text is included**, not the primitive character function.

Therefore 2200 versus 2200 is not a matched resource allowance.

The 47 versus 44 result cannot cleanly be interpreted as one system having greater capacity. It can reflect:

- delimiter accounting;
- formatting overhead;
- per-entry overhead;
- refusal boundary behavior;
- possibly different treatment of the final entry.

It is still valid as an **as-installed behavioral outcome** if those are genuinely the defaults. It is not valid as a normalized capacity comparison.

## Your âshipped defaultsâ claim is also false in code

You instantiate Hermes as:

```python
MemoryStore(memory_char_limit=cap, user_char_limit=cap)
```

At cap 2200 this sets:

- memory limit = 2200
- user limit = 2200

The declared shipped defaults are:

- memory limit = 2200
- user limit = 1375

Even if the user budget is irrelevant to this memory-only run, you cannot label that constructor invocation âshipped defaults.â Instantiate with no arguments for the actual default arm, or pass 2200 and 1375 explicitly.

The same scrutiny applies to Regentâs mirror budgets.

## Does this invalidate the raised arm?

Not as a storage-cap isolation, provided you assert that all 500 exact entries are present in both systems. Once neither cap binds, differences in cap accounting do not affect storage coverage.

But the raised arm still has the separate delivered-budget accounting defect. Equal storage does not rescue an unequal or fictional context budget.

## Prediction 4 is already compromised

Your equivalence margin for entries at 47 versus 44 is:

```text
max(2, 0.05 Ã 47) = 2.35
```

The observed difference is 3, which is outside the margin.

Because `entries_stored` is explicitly defined as a secondary metric, âno metric has been computed yetâ is inaccurate. You have observed at least one per-seed metric value. You have not computed the primary metric, but the broader claim is false.

---

# 5. The hard negatives are not mechanically established as hard

The builder does not assert semantic adjacency. It asserts that entries labeled `hard_negative` share a manually assigned topic.

This would pass:

```json
{
  "role": "hard_negative",
  "topic": "auth",
  "text": "bananas are yellow"
}
```

The assertion is therefore tautological. You label an item hard, then count hard labels.

More importantly, several negatives are not factually wrong answers to a sufficiently specified query. They concern a different subject:

- production versus staging;
- billing versus reporting service;
- platform versus security team;
- integration versus unit tests;
- audit versus access logs;
- Android versus iOS.

Those are adjacent facts, not necessarily false facts.

The queries are too underspecified:

```text
what is the db engine?
what is the auth?
what is the editor?
what is the queue?
```

âWhat is the db engine?â does not identify the billing service. âWhat is the auth?â does not identify the API. The query often omits precisely the entity distinction that separates gold from a negative.

## The deeper defect: truth is not retrievable here

You put contradictory claims into the same memory:

- API uses mutual TLS.
- API uses bearer tokens.
- API dropped mutual TLS.
- API uses mutual TLS is declared gold.

Nothing in the query tells the retriever which claim is true. There is no:

- timestamp;
- source authority;
- confidence;
- recency marker;
- explicit correction relation;
- provenance;
- validity interval.

The benchmark expects Regent to infer an externally declared truth that is absent from the indexed evidence.

Retrieval systems generally retrieve documents relevant to the query. A factually false document about exactly the requested entity and relation is still highly relevant for retrieval. Determining which claim is true is a verification, provenance or temporal-resolution task, not ordinary retrieval.

Consequently, demoting all contradictory statements below the declared gold is not evidence of semantic understanding. It may just be lexical luck.

## What can be proved without an LLM?

No finite corpus check can prove that a negative is âsemantically hardâ in the abstract. Hardness is relative to a retrieval method.

You can mechanically establish narrower properties:

1. Represent each fact structurally:
   - entity;
   - relation;
   - value;
   - polarity;
   - time;
   - source.
2. Generate negatives by frozen transformations:
   - same entity and relation, changed value;
   - same entity/relation/value, flipped polarity;
   - same relation/value, changed entity.
3. Assert exactly which slots match and differ.
4. Evaluate frozen non-LLM baselines:
   - BM25;
   - character n-gram TF-IDF;
   - word TF-IDF;
   - edit distance;
   - the same embedding model used by Regent, without Regentâs other lanes.
5. Define âhard for baseline Xâ using a predeclared rank criterion.

But you must first make the gold distinguishable. For example:

```text
Query: According to the current production configuration, which region
receives production deploys?

Gold: Current production configuration: production deploys go to eu-west-2.

Negative: Superseded configuration from 2024: production deploys went to
us-east-1.
```

Now a system can use âcurrent,â provenance or dates. In the present corpus, the intended truth is hidden solely in the annotation.

---

# 6. `by_text` is a real failure point

The filler suffix only prevents duplicate filler text. It does not prove global uniqueness.

You have no assertion that:

```python
len({e["text"] for e in corpus}) == len(corpus)
```

or the Rust equivalent.

Both harnesses collapse duplicates:

```python
id_of = {e["text"]: e["id"] for e in corpus}
```

and:

```rust
by_text.insert(text.to_owned(), id.clone());
```

The Regent version is worse because `by_text` is populated before knowing whether the entry was accepted. If two IDs share text and the later one is refused, retrieval of the accepted text can map to the refused ID.

Other failure modes:

- the store normalizes whitespace;
- trims text;
- normalizes Unicode;
- adds or removes formatting;
- deduplicates identical content;
- returns modified content;
- two graph nodes have identical content;
- a hit maps to `?nodeid`, which the harness silently permits.

The `?` fallback must not survive into a scored artifact. It is harness corruption, not a valid corpus ID.

## Required assertions

Before writing artifacts:

```python
texts = [e["text"] for e in corpus]
assert len(texts) == len(set(texts))
```

After loading each system:

- all delivered/ranked IDs must be known corpus IDs;
- no fallback ID is allowed;
- no duplicate ID is allowed unless the system genuinely emits duplicates and that behavior is explicitly scored;
- the delivered full set must equal the stored set when whole-store delivery is claimed;
- refused IDs must never appear;
- stored IDs must all be recoverable exactly.

Prefer stable entry IDs from the storage API over reverse mapping by content. If the API does not preserve external IDs, exact global uniqueness plus fatal assertions is the minimum acceptable workaround.

---

# 7. Hermes does not have constant recall across queries

Your premise here is wrong.

Hermes has the same **delivered set** for every query. Its recall is not necessarily constant because each query has a different gold ID.

For singleton gold:

```text
recall(q, B) = 1 if qâs gold is in the static prefix, otherwise 0
```

So a static prefix containing 7 of the 20 gold entries yields seven query recalls of 1 and thirteen of 0. The macro-average is 0.35.

That average is meaningful as:

> fraction of the 20 target facts covered by this one static context prefix

It should not be described as 20 independent retrieval decisions. Hermes made one context-selection decision and that one selection was evaluated against 20 target facts.

Regent makes 20 query-conditioned ranking decisions. The asymmetry is real and central to the comparison.

## Does the same objection apply to Regent?

Not identically.

- Hermes: one delivered prefix per seed and budget, evaluated against 20 targets.
- Regent: one delivered prefix per query per seed and budget.

Neither gives you 20 independent draws from a population because the corpus and topics are fixed. But Regent genuinely produces 20 distinct query-conditioned outputs, while Hermes does not.

Report:

- Hermes seed-level coverage of the 20 gold facts;
- Regent macro recall over the 20 queries;
- all 20 binary outcomes or ranks;
- no inferential claim based on treating those 20 as independent replications.

Your seed interpretation also needs work. Regentâs ranking may be nearly invariant to insertion order, while Hermesâs prefix is highly seed-dependent. The three seeds are therefore not equivalent sources of uncertainty across systems.

---

# 8. The majority-of-five-budgets rule is unsound and incomplete

It gives five equal votes to five highly correlated, nested thresholds without any justification that those budgets are equally important.

A system can win three irrelevant budgets and lose the one operational budget that matters. The score still calls it a winner.

The chosen grid also influences the winner. Adding two nearby budgets around one favorable part of the curve could reverse the majority without changing either system.

## The mapping has uncovered cases

The table does not define every possible five-budget outcome. Examples:

- 2 wins, 2 losses, 1 equivalent;
- 2 wins, 1 loss, 2 equivalent;
- 1 win, 2 losses, 2 equivalent.

None has:

- 3 wins;
- 3 losses;
- or 3 equivalences.

There is no score.

That alone means the scorer is not determined by the frozen protocol.

## Aggregation order is also not frozen

You have not specified whether to:

1. average queries within seed, then seeds, then compare;
2. average all 60 seed-query cells;
3. classify each seed at each budget, then vote across seeds;
4. classify each query, then average;
5. calculate scores per seed and then combine scores.

These can produce different winners.

You also have not specified whether scoring uses:

- raised arm only;
- shipped arm only;
- separate arm scores;
- means across arms;
- or the Â§5.4 refusal rule after separate scoring.

## Better alternatives

Pick an operational primary budget if one exists.

If budgets genuinely vary, freeze a budget distribution and calculate:

```text
expected recall = Î£ p(B) Ã recall(B)
```

Alternatively use a predeclared normalized area under the recall-budget curve, preferably with interpolation and weighting defined in advance. If context budgets are naturally multiplicative, integration over log-budget is more defensible than five equal point votes.

At minimum report the raw curve and do not destroy it by converting it into a 1â5 ordinal label.

## Your equivalence formula mostly collapses to an absolute margin

For recall and MRR, both bounded by 1:

```text
0.05 Ã max(a,b) <= 0.05
```

The absolute floor is also 0.05. Therefore the relative term never controls. Your elaborate denominator declaration has no effect for those metrics; equivalence is simply an absolute 0.05 margin.

That is not necessarily wrong, but the claimed denominator precision is cosmetic for the main metric.

---

# 9. Prediction 3 is outcome-falsifiable but causally invalid

The numerical statement is formally falsifiable:

- below 0.881: passes;
- at or above 0.881: fails.

But the explanation âhard negatives cost Regentâ is not tested by that comparison.

Between v2 and v3 you changed more than hard-negative difficulty:

- corpus;
- query set or query construction;
- number of queries;
- corpus size/composition;
- storage regime;
- possibly ranking candidate population;
- likely distribution of text lengths and topics.

A lower MRR cannot be attributed to hard negatives. A higher MRR does not show that the negatives were ineffective, because other changes may have improved performance.

## Required falsifiable test

Create a paired v3 ablation:

- same gold;
- same queries;
- same filler;
- same insertion order;
- same corpus size;
- same system configuration;
- one version with the three hard negatives;
- one version with matched neutral replacements or with those negatives removed under a predefined adjustment.

Then predict a delta:

```text
MRR_with_hard_negatives < MRR_without_hard_negatives
```

Better still, directly report how many hard negatives rank above their corresponding gold and their rank displacement.

Your current prediction only tests whether a new benchmark has a lower number than an old benchmark. It does not test the proposed cause.

---

# 10. MRR is underspecified and mishandles misses

You write:

> Undefined (not zero) when no gold was delivered.

For standard MRR, if a query has a judged relevant document but it is not retrieved within the evaluated list, reciprocal rank is **zero**, not undefined.

Undefined is appropriate when the query has no relevant document in the evaluation universe. That is not the case here: each query has a frozen gold entry.

Treating misses as undefined and dropping them creates survivorship bias. A system that retrieves gold for only one easy query can receive a high conditional MRR after the other nineteen failures are excluded.

You may report both:

- unconditional MRR, with misses as zero;
- conditional mean reciprocal rank among successful queries;
- retrieval/storage coverage.

But the scored end-to-end MRR must not silently exclude misses.

It is also unclear whether MRR is computed over:

- the full returned order;
- each budget prefix;
- only stored entries;
- or actual rendered delivery.

âDelivered orderâ conflicts with Regentâs full `k=500` ranked list and the later budget truncation.

---

# 11. Prediction 2 is badly calibrated

At the raised cap, Hermesâs formatted block is 24,432 characters.

Your B=4000 approximation permits 16,000 characters, only about 65% of that block before entry-boundary effects. That does not âdeliver most of the storeâ strongly enough to imply convergence within five recall points.

If Regent places nearly all gold near the front and Hermes exposes a roughly random 65% prefix, a gap around 0.35 is entirely plausible. Prediction 2 is not merely risky; its stated rationale contradicts your own smoke output.

If you want a convergence prediction, include a budget large enough to contain the complete exact rendered corpus for both systems. At that point both recalls should reach 1 in the raised arm, assuming full ranking and no rendering failures. That convergence would be entailed and therefore a sanity check, not an interesting prediction.

---

# 12. The gold-spread assertion does not establish spread

These assertions:

```python
assert spread[0] < N_CORPUS * 0.2
assert spread[-1] > N_CORPUS * 0.6
```

only prove that at least one gold is early and one is late.

The other eighteen gold entries could all be clustered together. Your comment claims much more than the code establishes.

Use frozen conditions such as:

- minimum gold count in each quintile;
- maximum allowed gap;
- minimum occupied bins;
- limits on prefix gold density at shipped-cap boundaries.

Better, emit and inspect the complete position vector for each seed rather than reducing it to `min..max`.

---

# 13. The shipped/raised disagreement rule is conceptually wrong

The two arms answer different questions:

- shipped arm: installed behavior under native, unmatched accounting semantics and binding storage limits;
- raised arm: context selection after storage saturation is removed.

There is no reason they should agree. In fact, disagreement is expected when storage capacity changes what can be delivered.

Declaring a criterion unscored whenever the arms disagree throws away the cleaner raised-arm answer because the intentionally confounded shipped arm says something else.

Report separate conclusions:

- shipped/default product behavior;
- raised/full-store context-selection behavior.

Do not require agreement between different estimands.

---

# 14. Your N/A rule conflicts with the primary comparison

You state:

> A criterion whose operation a system does not implement is N/A.

You also state:

> Hermesâs built-in memory has no retrieval.

Yet you intend to score Hermes against Regent on recall under ranked versus block delivery and describe the raised arm as isolating retrieval.

You must choose:

- If the criterion is **retrieval/relevance filtering**, Hermes is N/A.
- If the criterion is **gold coverage in delivered context**, both systems implement delivery and can be scored.

Renaming the operation is not cosmetic. It determines whether your own N/A rule applies.

The protocol currently tries to claim a retrieval win while avoiding the consequence of its retrieval N/A rule.

---

# 15. The nonexistent scorer is the largest remaining procedural hole

You froze prose, corpus and raw-output harnesses, but not the executable transformation that determines the result. The scorer is where most remaining discretion lives:

- exact tokenization;
- formatting overhead;
- boundary handling;
- partial entries;
- query averaging;
- seed averaging;
- NaN handling;
- MRR misses;
- equivalence application;
- arm combination;
- unresolved score patterns;
- malformed/unknown IDs;
- duplicate hits;
- incomplete ranked lists.

That is enough flexibility to reproduce the v2 problem even with a frozen metric name.

Before generating any primary result, commit a scorer and commit adversarial unit fixtures that establish:

1. exact token boundary behavior;
2. an entry exactly fitting the budget;
3. an entry exceeding the remaining budget by one token;
4. handling of a partial final entry;
5. duplicate IDs;
6. duplicate text;
7. unknown IDs;
8. refused IDs in results;
9. incomplete Regent rankings;
10. no-gold retrieval;
11. MRR miss equals zero;
12. all five-budget vote patterns, including currently uncovered patterns;
13. seed/query aggregation order;
14. equivalence exactly on the boundary;
15. arm disagreement behavior.

The scorer should fail closed, not silently continue.

---

# What must be fixed before any primary number is generated

These are blockers, not optional polish:

1. **Commit the scorer first**, with unit tests and fully frozen aggregation.
2. **Replace four-chars-per-token with exact tokenization of exact rendered context.**
3. **Decide whether this is retrieval or context allocation.**
   - Retrieval: Hermes N/A.
   - Context allocation: both scoreable, but stop calling it policy-free.
4. **Make truncation correspond to actual delivery behavior**, or label it explicitly as a synthetic evaluation prefix.
5. **Repair the hard-negative design.**
   - Structured generation and assertions;
   - fully specified queries;
   - truth/provenance/temporal evidence that makes gold distinguishable.
6. **Add a paired hard-negative ablation** if you want to attribute an MRR change to hard negatives.
7. **Assert global text uniqueness and complete ID recovery.**
8. **Assert complete, duplicate-free Regent rankings over exactly the stored set.**
9. **Treat missing gold as reciprocal rank zero**, not undefined.
10. **Replace or fully specify the five-budget scoring rule.**
11. **Freeze query/seed aggregation order.**
12. **Separate shipped and raised conclusions rather than requiring agreement.**
13. **Fix the Hermes shipped constructor arguments.**
14. **Correct the 20-versus-30 query contradiction.**
15. **Strengthen the gold-spread assertions.**
16. **Acknowledge that the stored-count metric has already been observed and that seed 11 is outside the frozen shipped-count margin.**

# Bottom line

v3 has finally isolated full-store versus capped-store behavior in the raised arm. That is the part you fixed. The measurement around it is still broken.

The current primary result would mostly measure:

> how Regentâs ranking of an artificially contradictory corpus compares with a random static prefix under an approximate, externally invented character budget.

That can become a legitimate benchmark, but it is not yet the policy-free retrieval measurement the protocol claims. The scorer, budget semantics, task definition and truth model must be repaired before you run it.

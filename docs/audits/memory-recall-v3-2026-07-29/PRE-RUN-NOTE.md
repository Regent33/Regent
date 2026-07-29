# v3 pre-run note — 2026-07-29

Written **before any metric exists**. The scorer has not been written. Git order
is the proof: protocol (`7960480`), corpus (`09ea9f2`), then this and the
harnesses, then — only after review — the scorer and the runs.

## Status: blocked on adversarial review, deliberately

The harnesses are written and smoke-tested. They have **not** been run across
the full matrix, and no metric has been computed, because the owner asked for
the benchmark code to be confirmed by the co-reviewer (GPT 5.6 sol) first.

Reviewing the harness *before* results exist is stronger than reviewing it
after: it removes any possibility that I reshaped the measurement once I saw a
number I disliked, which is exactly what the v1 and v2 reviews caught me doing.

The review could not be delivered: the local CLIProxyAPI is alive (25 models
listed) but its Codex provider has no auth —
`auth_unavailable: no auth available (providers=codex, model=gpt-5.6-sol)` —
and the fallback path reports `Usage credits are required for this model`.
That needs the owner's action. **No review has been fabricated or paraphrased,
and the full run stays blocked until a real one arrives.**

## Smoke results — stored counts only, no metrics

| arm | system | cap | stored | refused | payload |
|---|---|---|---|---|---|
| shipped | hermes | 2,200 | 44 | 456 | block 2,347 chars |
| shipped | regent | 2,200 | 47 | 453 | 47 vectors |
| raised | hermes | 32,000 | 500 | 0 | block 24,432 chars |
| raised | regent | 32,000 | 500 | 0 | 500 vectors |

**The raised arm does what v3 claims.** Both systems hold the entire 500-entry
corpus, so capacity is neutralised and the only remaining difference is
delivery. That is the regime v1 and v2 never reached.

## Concerns I am recording before the run, not after

Declared here so they cannot later look like excuses invented to explain a
result. Each is also in the review packet, in my own words, because a reviewer
who has to find them unaided is a reviewer I have wasted.

1. **The shipped arm's accounting may not be like-for-like.** At the same
   nominal cap of 2,200, Regent stored 47 and Hermes 44. Regent counts
   `chars().count()` over entry text; Hermes counts `len()` of the
   delimiter-joined string, so it charges itself for delimiters Regent does not
   count. That is a ~7% difference in effective capacity at identical nominal
   settings. It does **not** touch the raised arm — where both store everything
   — but any shipped-arm capacity comparison is confounded by it and must say
   so.

2. **`recall_at_budget` may have moved the confound rather than killed it.**
   Each system truncates in "its own natural order" — Regent by rank, Hermes by
   block order. That is defensible, but it is also the assumption that decides
   the result, and I chose it. If the reviewer says it hands Regent the win by
   construction, that objection stands and the metric is not policy-free.

3. **Hard negatives are plausibly hard, not provably hard.** I wrote them to be
   semantically adjacent and factually wrong. Nothing in the harness *proves* a
   retrieval system finds them confusable. A stronger design would demonstrate
   difficulty independently.

4. **Hermes's per-query recall is constant**, because it delivers the same set
   for every query. Averaging that across 20 queries may be reporting one
   observation twenty times. Whether the same objection applies to Regent — whose
   deliveries do vary — is a question for the reviewer, not for me.

None of these are resolved. They are on the record because the run has not
happened yet.

# v5 killed at design stage — the renderer is not a template

**Verdict, verbatim:** *"No: v5 is not yet executable to a unique measurement as
frozen. I would stop before executing the scorer."* … *"v5 as written should not
be run."*

Fifth design, fifth kill. The scorer was never executed and no metric was
computed. Full review: `../../../../scratchpad` is transient; the text is
reproduced under `design-review-5.6-sol-full.md`.

What is different this time, and worth recording because four of the five kills
have been for the same class of fault:

> *"You are converging on something measurable. The target bijection, seed
> aggregation, raised-arm assertion, whole-block tokenization, explicit budget
> curve, and narrowed claims are all real improvements."*

The three v4 killers stayed dead. Nothing that was fixed came back.

---

## The four fatals, each verified against my own artifacts

### F1 — Hermes's header is data-dependent, so §5's template contract is wrong

§5 has each harness emit a static `prefix / separator / suffix` and has the
scorer rebuild every candidate prefix from it. Hermes's prefix, read off the
product:

```
══════════════════════════════════════════════
MEMORY (your personal notes) [17% — 34,599/200,000 chars]
══════════════════════════════════════════════
```

The percentage and the character count are **live values computed from the whole
store**. Reusing that string for a synthesized 5-entry prefix produces a block
the product would never emit — one claiming 34,599 chars above five records —
and its token length is not even constant across candidate `k`, because the
digits change. Every token count at every boundary is affected.

**The repair is to stop reconstructing native rendering at all**: the harness
emits the exact opaque block the product renders for the first `k` records, for
each `k`, and the scorer only tokenizes complete strings. That also disposes of
the BPE-additivity problem without the scorer imitating either renderer.

### F2 — Regent's provenance is an unfrozen per-entry input

The rendered line, verbatim from the smoke run:

```
- [memory | agent_inferred | trust 0.7] "CURRENT (2026-06): the billing service's database engine is postgres 16"
```

`agent_inferred` and `trust 0.7` are charged against the 600-token budget on
every entry of every query, and **the protocol never specifies either**. It
happens to be constant here; nothing in the frozen text makes it so, and nothing
forbids a value that varies by seed or carries a path. A harness author had to
inherit it through an unstated API path — which is precisely the class of open
decision that killed v4.

### F3 — `mrr` is contradictory for Hermes

§4.2 defines `mrr` over "the full ranking" for every harness. §6.3 says Hermes is
N/A on ranking quality. §7 says cross-system ranking comparison is impossible.
A static delivery order still has a well-defined target position, so a scorer
author must pick between two frozen instructions. My scorer computes it. The
protocol both requires and forbids that.

### F4 — Regent does not return an exhaustive ranking, and §8 asserts that it does

§8 requires "every stored id exactly once in a full ranking". Measured on the
smoke run at seed 101, raised arm: `retrieve(query, 500)` returns a **mean of
29.0 entries (min 20, max 36)**, not 500. Regent's seed lanes are bounded; the
tail does not exist.

So the assertion I wrote would have **aborted the whole matrix on the first
query**, and had I written the Rust harness to fill the tail instead, a synthetic
ordering of 470 records would have decided delivery at the larger budgets. The
review's preferred repair is the honest one: treat the returned list as the
product's complete output, score a missing target as delivery 0 and reciprocal
rank 0, and delete the exhaustiveness assertion.

---

## The finding I did not see, and it undoes stratum D's stated purpose

D was built so the currency signal could not be matched lexically: query says
*"as of now"*, record says `Status: ACTIVE`. The protocol then claims a system
*"must carry the mapping semantically or lose the currency signal."*

**That is false, and the corpus is what makes it false.** Every role carries a
fixed date:

| role | date |
|---|---|
| gold | `2026-06` |
| rejected | `2025-11` |
| superseded | `2024-03` |

Entity and relation are strong lexical filters *by design*, so a system can find
the three same-entity/same-relation records and **take the largest date**. No
status vocabulary is involved. Worse, the correlation is global and constant, so
the date pattern leaks the marker ontology across the whole corpus regardless of
how the three synonym maps are rotated — which also answers my question about
rotation: *"Rotation is not the property you need; controlled cross-classification
is."*

D still tests something — removal of exact status-token overlap — but not what I
said it tests. The review's less invasive repair is to narrow the claim and add a
deterministic **entity/relation + latest-date** baseline, alongside the frozen
status-thesaurus baseline. If Regent cannot beat those, that is the finding.

---

## On native rendering — I asked whether it was a formatting contest, and it isn't

I flagged that charging Regent for provenance metadata on every entry might make
the primary metric a formatting contest. The answer was no, with a warning:

> *"do not strip native formatting from the primary metric merely because it
> hurts Regent. Stripping it would change the estimand after discovering an
> unfavorable product behavior."*

Native rendering stays primary. What gets added is a **decomposition**:
standardized raw-record rendering over the product's own order as a labelled
secondary, plus formatting diagnostics (header tokens, metadata tokens per
entry, wrapper-to-payload ratio). One condition attached: `render_recall` must be
proven to be the production context path and not a debug serializer.

---

## Contamination disclosure

While verifying F4 I read the ranks of all 20 targets in Regent's returned list
at seed 101: **every one is at rank 0, 1 or 2.** That is reciprocal-rank
information, which is a protocol metric under §4.2 — so this is not a
"diagnostic", and calling it one would repeat exactly the v3 error where I
published `entries_stored` under a heading claiming no metric had been computed.

It scores nothing: it is one seed, one arm, under a protocol that is now dead.
But **I have now seen ranking information before the design was approved**, and
any v6 must be written knowing I know it. That disclosure belongs in the v6
review packet, not buried here.

---

## The seven repairs, as given

1. Make native rendering opaque and exact for every candidate prefix.
2. Freeze Regent's provenance construction.
3. Resolve the Hermes MRR contradiction.
4. Define finite vs exhaustive Regent output (finite is preferred).
5. Operationalize baselines 4, 5, 8, 10, 11 and 12 — including the status
   thesaurus named honestly, and a full runtime lockfile for MiniLM.
6. Add the latest-date baseline.
7. Define and assert **query-level** conjunction breakage in the intervention
   corpus; preserved marginals do not prove the conjunction was broken, only that
   the slots were moved. Until then P5 is renamed or unfalsifiable.

Also: `target_too_long` must be tested against `prefix + target + suffix`, not
the target alone; "maximal prefix" must become the **sequential** complete-entry
prefix, since whole-string BPE is not guaranteed monotonic across candidates; and
`k = 0` rendering must be frozen as whatever the product actually sends.

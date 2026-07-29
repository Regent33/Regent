# Paired process-cost measurement — protocol

**Frozen before the harness exists.** Git order is the proof.

Scope: the criteria that can be measured **deterministically, paired, with no
model call and no corpus** — process cost. This is a different family from the
memory-recall protocols (v1–v5, all withdrawn) and shares none of their
machinery deliberately: five designs died on corpus and ranking semantics, and
none of that applies here.

---

## 1. What this measures, and what it cannot

> **Time and memory each product spends before and around one unit of work,
> measured on the same machine, same OS, same disk, back to back.**

Covers, from the audit's criteria list:

- **Performance (b) latency per request** — the audit scored this Regent on
  *structural* grounds ("Rust + Tokio, no GIL") with no measurement. That is an
  inference from language choice, and it is exactly the kind of claim this
  measures instead of assuming.
- **Performance (e) resource utilization** — same, scored Regent structurally.
- **Performance (c) throughput** — partially: single-process op rate only. It
  does **not** measure multi-tenant operation, which is what the audit actually
  credited Hermes for. That part stays unscored.

**What it does not measure and will not be read as:** task success, quality of
output, anything requiring a model. A fast wrong answer is not a better answer.

## 2. The two levels, because they answer different questions

**L1 — product readiness.** Wall-clock from process launch to the point where
the product can accept its first request.

- Regent: `regent-deacon.exe` launch → first JSON-RPC response on stdio.
- Hermes: `python cli.py` import to a prompt-ready state.

This is what a user waits for. It is **not** an algorithm comparison: it prices
one product's compiled binary against another's interpreted import graph, and
that is the honest framing.

**L2 — same operation, each in its native stack.** Both systems perform an
identical unit of memory work: open a store, write *N* entries, render the
injection block, read it back.

- Regent: `regent-store` + `regent-graph` through the same public API the deacon
  uses.
- Hermes: `tools.memory_tool.MemoryStore` through the same API its memory tool
  uses.

Neither is patched. Both are driven through their own public surface.

## 3. Frozen measurement rules

- **Repetitions: 11 per cell.** The first is discarded as warm-up and the
  remaining 10 are reported as **median and full range**, never as a mean alone.
- **Order: interleaved** (regent, hermes, regent, hermes …), so machine drift
  hits both arms equally rather than accumulating in whichever ran second.
- **Timer:** `time.perf_counter()` around a fully separate OS process for L1;
  in-process for L2, measured in each stack's own clock.
- **Memory:** peak working set of the child process, sampled by the parent, in
  bytes. Reported for L1 only — L2 shares a process with the harness.
- **N = 100 entries**, each a fixed 60-character record, frozen in the harness.
  Both systems' caps are raised to 200,000 characters through their own public
  constructor so neither refuses a write. Asserted: both store all 100.
- **Cold vs warm disk is not controlled.** Interleaving is the mitigation, and
  the full range is published so a reader can see the spread.

## 4. Equivalence and scoring

`|a − b| <= 10%` of `max(a, b)` is a **tie**. Timing on a shared desktop under a
live session is not precise enough to justify a tighter margin, and a margin
chosen after seeing the numbers is not a margin.

Scored per criterion, **A = Regent, B = Hermes**, on the median:

| condition | score |
|---|---|
| A faster/smaller by > 2× | **5** |
| A better, beyond the tie margin, under 2× | **4** |
| within the tie margin | **3 — tie, both sides** |
| B better, beyond the tie margin, under 2× | **2** |
| B faster/smaller by > 2× | **1** |

## 5. Declared in advance

1. **L1 is not a fair algorithm comparison and will not be reported as one.**
   Python import cost is a property of the deployment, not of the memory design.
2. **Regent is expected to win L1 by a large factor.** Predicting it is not the
   same as it being interesting; a compiled binary beating a 780 KB Python
   import graph is a prediction with no diagnostic value, and it is labelled a
   sanity check, not a finding.
3. **L2 is the informative one**, because both systems do identical work and the
   only difference is the implementation of that work.
4. **A Hermes win anywhere is reported unchanged.** Two of the five memory
   protocols died because I reached for a secondary analysis to overturn a
   primary result I disliked.
5. The harness asserts both systems actually stored all 100 entries before any
   timing is recorded. An arm that silently refused writes would look fast.

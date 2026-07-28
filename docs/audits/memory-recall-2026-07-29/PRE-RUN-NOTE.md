# Pre-run note — 2026-07-29, before the harness was built

Kept separate from `memory-recall-protocol-2026-07-29.md` so the frozen
protocol is never edited. Written **before any system was run**; git order is
the proof.

## Both systems cap the entry path at the same numbers

Verified in each source, not inherited from the audit:

| | Regent | Hermes |
|---|---|---|
| memory budget | `2_200` (`regent-graph/.../orchestrators.rs:34`) | `2200` (`tools/memory_tool.py:167`) |
| user budget | `1_375` (`orchestrators.rs:35`) | `1375` (same line) |

Regent's are byte-identical ports — the plan already records this, and it holds
on re-check. `GraphMemory::add_entry` returns `BudgetExceeded` past the limit
exactly as Hermes's `add` returns a refusal.

## What this does to the pilot

The protocol (§5) already covers it: the corpus goes in through each system's
own memory-add path, and **a refused write counts as a miss for every query
whose gold set includes it**. That rule was predeclared, so it applies
unchanged. Nothing here alters the protocol.

But it makes **prediction 2 probably wrong before the run starts**, and that is
worth saying now rather than after:

> *"**d** — Regent wins; Hermes refuses writes past its cap."*

Both refuse, at the same number. So (d) on the entry path looks like a **tie**,
not a Regent win. I am leaving the prediction as written — a prediction you
amend once you can see it failing is not a prediction.

## The consequence for (b) and (e)

At N = 60 and N = 200 both systems will hold the *same* storable subset — the
first ~2,200 chars — so any difference in recall or precision at those sizes is
**purely ranking**, not capacity. That is a cleaner measurement of (b) and (e)
than the protocol anticipated, and it is the honest reading.

Regent's `nodes` table is genuinely unbounded and its retrieval ranks over all
of it (the audit's point about `kind=constitution` at 13,764 chars). That is a
real architectural difference — but it is reached by a *different* write path
than the memory tool a user's entries go through, so folding it into these
numbers would be comparing Regent's back door with Hermes's front door. If it is
measured, it will be reported as its own clearly-labelled datum and will not be
used to score (b), (c), (d) or (e).

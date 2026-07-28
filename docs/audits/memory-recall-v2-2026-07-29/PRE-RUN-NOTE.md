# v2 pre-run note — 2026-07-29, before the harnesses were run

Separate file so the frozen v2 protocol is never edited. Written after building
the corpus and **before any system was run**; git order is the proof.

## What building the corpus revealed

With a seeded shuffle of 200 entries and a ~2,200-char cap, only **2–5 of the
15 gold entries survive** into the store (seed 11: 2, seed 22: 5, seed 33: 2).

That is the failure v1 never reached, and it is correct — the predeclared
"refused write counts as a miss" rule now actually fires. But it has a
consequence the protocol did not anticipate:

**Most queries become unanswerable for *both* systems**, because their gold
entry was never stored. Those queries then measure the cap, not retrieval, and
they would drown the very signal criteria (b) and (e) exist to detect.

## The stratification, declared now

`delivered_recall` as frozen in §4 stays **primary and unchanged**:

> `delivered_recall_all` = |gold ∩ delivered| / |gold|, over every query.

Added alongside it, reported separately, never replacing it:

> `delivered_recall_stored` = the same ratio computed **only over gold entries
> the system actually stored**.

The first answers "did the memory system give the model the fact?" — which
capacity dominates. The second answers "given the fact was in the store, did the
system surface it?" — which is retrieval alone.

This is a **stratification, not a metric swap**: no frozen number changes, and
both are published. It is declared here because rule 4 forbids changing metrics
*after a result is seen*, and nothing has been run yet.

## Why this is not the v1 sin repeated

In v1 I replaced `recall@k` with a kinder metric *after* seeing that the frozen
one embarrassed a system. Here the frozen metric is untouched and still primary;
a second, narrower one is added before any number exists, for a reason that is
visible in the corpus rather than in the results.

If `delivered_recall_stored` turns out to flatter Regent, that is not evidence
for Regent — it is only evidence about retrieval given storage, and the write-up
must say so in those words.

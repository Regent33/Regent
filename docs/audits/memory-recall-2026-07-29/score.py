#!/usr/bin/env python3
"""Scores the paired memory-recall pilot from raw runs. The only place metrics
are computed, so both systems are treated identically by construction.

Two framings are reported because the systems differ in kind, and the protocol
said so before the run:

* **@k** — both truncated to the same k. Mechanically symmetric, but Hermes's
  order is FILE order, not relevance: it does not rank. Read it as "what the
  first k items happen to be", not as a retrieval-quality comparison.
* **all** — everything that actually reaches the model. This is the honest
  comparison of what the prompt carries: Regent's ranked top-10 vs Hermes's
  entire block.

A gold entry the store refused is simply absent from `returned`, so the
predeclared "refused write counts as a miss" rule needs no special case.

  score.py <dir-with-runs>
"""

import json
import pathlib
import sys

runs = pathlib.Path(sys.argv[1])
KS = (5, 10)


def load(system, n):
    return json.loads((runs / f"{system}-n{n}.json").read_text(encoding="utf-8"))


def metrics(queries, k=None):
    recalls, precisions = [], []
    for q in queries:
        gold = set(q["gold"])
        returned = q["returned"] if k is None else q["returned"][:k]
        hit = len(gold & set(returned))
        recalls.append(hit / len(gold) if gold else 0.0)
        precisions.append(hit / len(returned) if returned else 0.0)
    return sum(recalls) / len(recalls), sum(precisions) / len(precisions)


def by_kind(queries, k):
    out = {}
    for kind in ("lexical", "paraphrase", "distractor"):
        subset = [q for q in queries if q["kind"] == kind]
        out[kind] = metrics(subset, k)
    return out


print("Paired memory-recall pilot — criteria b, c, d, e\n")

print("(d) storage capacity — entries the store accepted")
print(f"{'N':>5}  {'Regent':>18}  {'Hermes':>18}")
for n in (20, 60, 200):
    r, h = load("regent", n), load("hermes", n)
    print(f"{n:>5}  {len(r['stored']):>8} / {n:<7}  {len(h['stored']):>8} / {n:<7}")

print("\n(b) recall & (e) precision — @k, both truncated the same way")
for k in KS:
    print(f"\n  k={k}")
    print(f"  {'N':>5}  {'Regent R':>9} {'Regent P':>9}  {'Hermes R':>9} {'Hermes P':>9}")
    for n in (20, 60, 200):
        rr, rp = metrics(load("regent", n)["queries"], k)
        hr, hp = metrics(load("hermes", n)["queries"], k)
        print(f"  {n:>5}  {rr:>9.3f} {rp:>9.3f}  {hr:>9.3f} {hp:>9.3f}")

print("\n(b)/(e) — ALL of what reaches the model (Regent top-10 vs Hermes whole block)")
print(f"{'N':>5}  {'Regent R':>9} {'Regent P':>9} {'items':>6}  {'Hermes R':>9} {'Hermes P':>9} {'items':>6}")
for n in (20, 60, 200):
    r, h = load("regent", n), load("hermes", n)
    rr, rp = metrics(r["queries"])
    hr, hp = metrics(h["queries"])
    ri = sum(len(q["returned"]) for q in r["queries"]) / len(r["queries"])
    hi = sum(len(q["returned"]) for q in h["queries"]) / len(h["queries"])
    print(f"{n:>5}  {rr:>9.3f} {rp:>9.3f} {ri:>6.1f}  {hr:>9.3f} {hp:>9.3f} {hi:>6.1f}")

print("\n(b) by query kind, k=10 — where ranking actually earns its keep")
for n in (20, 200):
    print(f"\n  N={n}")
    r, h = by_kind(load("regent", n)["queries"], 10), by_kind(load("hermes", n)["queries"], 10)
    print(f"  {'kind':>11}  {'Regent R':>9} {'Regent P':>9}  {'Hermes R':>9} {'Hermes P':>9}")
    for kind in ("lexical", "paraphrase", "distractor"):
        print(
            f"  {kind:>11}  {r[kind][0]:>9.3f} {r[kind][1]:>9.3f}  {h[kind][0]:>9.3f} {h[kind][1]:>9.3f}"
        )

print("\n(c) recall speed — median ms per query")
print(f"{'N':>5}  {'Regent':>10}  {'Hermes':>10}")
for n in (20, 60, 200):
    for system in ("regent", "hermes"):
        times = sorted(q["ms"] for q in load(system, n)["queries"])
        globals()[f"_{system}"] = times[len(times) // 2]
    print(f"{n:>5}  {_regent:>10.3f}  {_hermes:>10.3f}")  # noqa: F821

print("\nContext cost — chars of memory reaching the model per turn")
for n in (20, 60, 200):
    h = load("hermes", n)
    print(f"  N={n:>3}  Hermes block: {h['block_chars']} chars (every turn, every query)")

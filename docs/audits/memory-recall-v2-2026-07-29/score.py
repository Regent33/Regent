#!/usr/bin/env python3
"""Scores v2. The only place metrics are computed.

Implements exactly what the v2 protocol froze, plus the stratification declared
in PRE-RUN-NOTE.md. No metric is defined here that was not written down first.

  score.py <dir-with-runs>
"""

import json
import pathlib
import statistics
import sys

runs = pathlib.Path(sys.argv[1])
HERE = pathlib.Path(__file__).parent
NS = (10, 20, 30, 200)
SEEDS = (11, 22, 33)

corpus = {e["id"]: e["text"] for e in json.loads((HERE / "corpus.json").read_text(encoding="utf-8"))}

# Predeclared equivalence margin (protocol v2 §2.1).
def tie(a, b):
    return abs(a - b) <= max(2, 0.05 * max(a, b))


def load(system, n, seed):
    return json.loads((runs / f"{system}-n{n}-s{seed}.json").read_text(encoding="utf-8"))


def stats(run):
    stored = set(run["stored"])
    rec_all, rec_stored, waste_n, waste_chars, first_rank = [], [], [], [], []
    for q in run["queries"]:
        gold = set(q["gold"])
        delivered = q["returned"]
        hit = gold & set(delivered)
        rec_all.append(len(hit) / len(gold))
        # Stratified: only gold this system actually stored (PRE-RUN-NOTE).
        gold_stored = gold & stored
        if gold_stored:
            rec_stored.append(len(hit & gold_stored) / len(gold_stored))
        noise = [d for d in delivered if d not in gold]
        waste_n.append(len(noise))
        waste_chars.append(sum(len(corpus.get(d, "")) for d in noise))
        pos = next((i + 1 for i, d in enumerate(delivered) if d in gold), None)
        if pos:
            first_rank.append(pos)
    return {
        "rec_all": statistics.mean(rec_all),
        "rec_stored": statistics.mean(rec_stored) if rec_stored else float("nan"),
        "waste_n": statistics.mean(waste_n),
        "waste_chars": statistics.mean(waste_chars),
        "mrr": statistics.mean(1 / r for r in first_rank) if first_rank else 0.0,
        "distinct_orders": len({tuple(q["returned"]) for q in run["queries"]}),
        "queries": len(run["queries"]),
    }


def mean_over_seeds(system, n, key):
    """Mean over the seeds where the metric is DEFINED.

    `rec_stored` is undefined for a seed in which no query's gold survived
    storage. Averaging that NaN with the others propagated NaN across the whole
    row, so the first published table showed a nonzero `delivered_recall_all`
    beside an N/A stored recall and read as self-contradictory. A reviewer filed
    it as impossible data; the data were right, the aggregation was wrong.
    """
    values = [stats(load(system, n, s))[key] for s in SEEDS]
    defined = [v for v in values if v == v]  # NaN != NaN
    return statistics.mean(defined) if defined else float("nan")


def seeds_defined(system, n, key):
    return sum(1 for s in SEEDS if (lambda v: v == v)(stats(load(system, n, s))[key]))


print("v2 — 3 seeds, mean unless stated\n")

print("(d) entries stored — predeclared margin: tie if diff <= max(2, 5%)")
print(f"{'N':>5}  {'Regent':>8}  {'Hermes':>8}  {'verdict':>8}")
for n in NS:
    r = statistics.mean(len(load("regent", n, s)["stored"]) for s in SEEDS)
    h = statistics.mean(len(load("hermes", n, s)["stored"]) for s in SEEDS)
    print(f"{n:>5}  {r:>8.1f}  {h:>8.1f}  {'TIE' if tie(r, h) else 'differs':>8}")

print("\ndelivered_recall_all (frozen, primary) — over ALL gold, stored or not")
print(f"{'N':>5}  {'Regent':>8}  {'Hermes':>8}")
for n in NS:
    print(f"{n:>5}  {mean_over_seeds('regent', n, 'rec_all'):>8.3f}  {mean_over_seeds('hermes', n, 'rec_all'):>8.3f}")

print("\ndelivered_recall_stored (stratified, declared pre-run) — retrieval given storage")
print("  seeds = how many of 3 stored any gold; the rest are UNDEFINED, not zero")
print(f"{'N':>5}  {'Regent':>8} {'seeds':>6}  {'Hermes':>8} {'seeds':>6}")
for n in NS:
    print(
        f"{n:>5}  {mean_over_seeds('regent', n, 'rec_stored'):>8.3f} {seeds_defined('regent', n, 'rec_stored'):>6}"
        f"  {mean_over_seeds('hermes', n, 'rec_stored'):>8.3f} {seeds_defined('hermes', n, 'rec_stored'):>6}"
    )

print("\n(e) waste — non-gold entries delivered per query, and their chars")
print(f"{'N':>5}  {'Regent n':>9} {'Regent ch':>10}  {'Hermes n':>9} {'Hermes ch':>10}")
for n in NS:
    print(
        f"{n:>5}  {mean_over_seeds('regent', n, 'waste_n'):>9.1f} {mean_over_seeds('regent', n, 'waste_chars'):>10.0f}"
        f"  {mean_over_seeds('hermes', n, 'waste_n'):>9.1f} {mean_over_seeds('hermes', n, 'waste_chars'):>10.0f}"
    )

print("\n(e) rank signal — does delivered ORDER depend on the query?")
print(f"{'N':>5}  {'Regent distinct':>16} {'Regent MRR':>11}  {'Hermes distinct':>16} {'Hermes MRR':>11}")
for n in NS:
    rs = stats(load("regent", n, 11))
    hs = stats(load("hermes", n, 11))
    print(
        f"{n:>5}  {rs['distinct_orders']:>7}/{rs['queries']:<8} {rs['mrr']:>11.3f}"
        f"  {hs['distinct_orders']:>7}/{hs['queries']:<8} {hs['mrr']:>11.3f}"
    )

print("\n(c) per-turn context cost — memory chars entering the prompt")
for n in NS:
    h = statistics.mean(load("hermes", n, s)["block_chars"] for s in SEEDS)
    print(f"  N={n:>3}  Hermes block {h:>6.0f} chars, unconditional, every turn")
print("  Regent: frozen block + retrieval only on turns that call the tool (not measured here)")

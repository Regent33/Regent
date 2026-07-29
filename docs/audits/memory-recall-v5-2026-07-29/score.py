#!/usr/bin/env python3
"""The v5 scorer. Protocol §4, §5, §6, §7.

Tokenization and truncation happen HERE and nowhere else, for both systems and
every baseline. The harnesses emit ordered ids, rendered text and a join
template, and no numbers at all.

    python score.py <artifacts-dir> <runs-dir> <out.json>
"""

import json
import pathlib
import random
import sys

import tiktoken

import baselines
import spec

ART, RUNS, OUT = (pathlib.Path(p) for p in sys.argv[1:4])
ENC = tiktoken.get_encoding("cl100k_base")

queries = json.loads((ART / "queries.json").read_text(encoding="utf-8"))
targets = json.loads((ART / "targets.json").read_text(encoding="utf-8"))
stratum_of = {q["id"]: q["stratum"] for q in queries}


def cost_of(chunks, tmpl):
    """§5 — tokenize the WHOLE joined prefix. BPE is not additive across joins."""
    if not chunks:
        return 0
    body = tmpl["separator"].join(chunks)
    return len(ENC.encode(tmpl["prefix"] + body + tmpl["suffix"]))


def admitted_count(chunks, tmpl, budget):
    """Maximal complete-entry prefix. Cost is monotone in k, so binary search."""
    lo, hi = 0, len(chunks)
    while lo < hi:
        mid = (lo + hi + 1) // 2
        if cost_of(chunks[:mid], tmpl) <= budget:
            lo = mid
        else:
            hi = mid - 1
    return lo


def evaluate(ranked, chunks, tmpl, qid, stored):
    """target_delivery at every budget, mrr, and why any miss happened.

    `chunks[i]` is the rendered text of `ranked[i]` — each system's OWN
    rendering, metadata and all, because §5 makes formatting part of the
    capability being measured.
    """
    target = targets[qid]
    pos = ranked.index(target) if target in ranked else None
    t_cost = cost_of([chunks[pos]], tmpl) if pos is not None else None
    out = {"rank": pos, "mrr": 0.0 if pos is None else 1.0 / (pos + 1),
           "target_tokens": t_cost, "delivery": {}, "diagnostics": {}}
    for b in spec.BUDGETS:
        admitted = admitted_count(chunks, tmpl, b)
        delivered = pos is not None and pos < admitted
        if delivered:
            cause = None
        elif target not in stored:
            cause = "admission"
        elif t_cost is not None and t_cost > b:
            cause = "target_too_long"
        else:
            cause = "rank"
        out["delivery"][str(b)] = int(delivered)
        out["diagnostics"][str(b)] = {
            "entries_admitted": admitted, "cause": cause,
            "prefix_tokens": cost_of(chunks[:admitted], tmpl),
        }
    return out


def mean(xs):
    return sum(xs) / len(xs) if xs else 0.0


def aggregate(per_seed):
    """§4.1 — query -> seed -> mean over seeds. Never the seed mean alone."""
    return {
        "per_seed": per_seed,
        "mean": mean(list(per_seed.values())),
        "min": min(per_seed.values(), default=0.0),
        "max": max(per_seed.values(), default=0.0),
    }


def run_system(path):
    run = json.loads(path.read_text(encoding="utf-8"))
    stored = set(run["stored"])
    tmpl = {k: run["template"][k] for k in ("prefix", "separator", "suffix")}
    for q in run["queries"]:
        assert len(q["ranked"]) == len(q["rendered"]), \
            f"{path.name}: rendered list is not parallel to the ranking"
    return run, {q["id"]: evaluate(q["ranked"], q["rendered"], tmpl, q["id"], stored)
                 for q in run["queries"]}


def embed_fn():
    from sentence_transformers import SentenceTransformer
    model = SentenceTransformer("all-MiniLM-L6-v2")
    return lambda texts: model.encode(list(texts), normalize_embeddings=True,
                                      show_progress_bar=False)


def main():
    encode = embed_fn()
    results = {"tokenizer": "cl100k_base", "tiktoken": tiktoken.__version__,
               "budgets": spec.BUDGETS, "primary_budget": spec.PRIMARY_BUDGET,
               "systems": {}, "baselines": {}, "seeds": spec.SEEDS}

    for arm in ("shipped", "raised"):
        for corpus_tag in ("base", "intervention"):
            for system in ("regent", "hermes"):
                key = f"{system}/{arm}/{corpus_tag}"
                per_query, per_seed_all = {}, {}
                for seed in spec.SEEDS:
                    p = RUNS / f"{system}-{arm}-{corpus_tag}-s{seed}.json"
                    if not p.exists():
                        continue
                    run, ev = run_system(p)
                    per_query[seed] = ev
                    per_seed_all[seed] = run
                if not per_query:
                    continue
                results["systems"][key] = summarise(per_query)

    # §7 — baselines, on the base corpus, raised arm population, shared renderer.
    for seed in spec.SEEDS:
        p = RUNS / f"regent-raised-base-s{seed}.json"
        if not p.exists():
            continue
        run = json.loads(p.read_text(encoding="utf-8"))
        ids = list(run["stored"])
        raw = run["raw"]
        texts = [raw[i] for i in ids]
        tmpl = baselines.SHARED_RENDERER
        rank_sets = baselines.build(ids, texts, queries, encode, random.Random(seed))
        rank_sets["regent_lane_fts"] = {q["id"]: q["lane_fts"] for q in run["queries"]}
        rank_sets["regent_lane_vec"] = {q["id"]: q["lane_vec"] for q in run["queries"]}
        rank_sets["regent_fused"] = {q["id"]: q["ranked"] for q in run["queries"]}
        orc = baselines.oracles(ids, texts, queries, targets,
                                lambda l: cost_of([raw[i] for i in l], tmpl),
                                spec.PRIMARY_BUDGET)
        rank_sets["oracle_static"] = orc["oracle_static"]
        rank_sets["oracle_conditioned"] = orc["oracle_conditioned"]

        for name, ranks in rank_sets.items():
            slot = results["baselines"].setdefault(name, {})
            slot[seed] = {qid: evaluate(r, [raw[i] for i in r], tmpl, qid, set(ids))
                          for qid, r in ranks.items()}

    results["baselines"] = {n: summarise(v) for n, v in results["baselines"].items()}
    results["score"] = score_table(results)
    OUT.write_text(json.dumps(results, indent=1) + "\n", encoding="utf-8")
    report(results)


def summarise(per_seed_queries):
    """Collapse {seed: {qid: eval}} into the frozen aggregation, split by stratum."""
    out = {"delivery": {}, "delivery_L": {}, "delivery_D": {}, "mrr": {},
           "mrr_L": {}, "mrr_D": {}, "causes": {}, "per_query": {}}
    for b in spec.BUDGETS:
        for tag, keep in (("", None), ("_L", "L"), ("_D", "D")):
            per_seed = {
                s: mean([e["delivery"][str(b)] for q, e in evs.items()
                         if keep is None or stratum_of[q] == keep])
                for s, evs in per_seed_queries.items()}
            out[f"delivery{tag}"][str(b)] = aggregate(per_seed)
        causes = {}
        for evs in per_seed_queries.values():
            for e in evs.values():
                c = e["diagnostics"][str(b)]["cause"]
                causes[c or "delivered"] = causes.get(c or "delivered", 0) + 1
        out["causes"][str(b)] = causes
    for tag, keep in (("", None), ("_L", "L"), ("_D", "D")):
        out[f"mrr{tag}"] = aggregate({
            s: mean([e["mrr"] for q, e in evs.items()
                     if keep is None or stratum_of[q] == keep])
            for s, evs in per_seed_queries.items()})
    out["per_query"] = {s: {q: {"rank": e["rank"],
                                "delivered@600": e["delivery"][str(spec.PRIMARY_BUDGET)]}
                            for q, e in evs.items()}
                        for s, evs in per_seed_queries.items()}
    return out


def score_table(results):
    """§6.2 — A = Regent, B = Hermes, at B* on the raised arm, on target_delivery."""
    b = str(spec.PRIMARY_BUDGET)
    a_key, b_key = "regent/raised/base", "hermes/raised/base"
    if a_key not in results["systems"] or b_key not in results["systems"]:
        return {"scored": False, "reason": "raised-arm runs missing"}
    a = results["systems"][a_key]["delivery"][b]["mean"]
    h = results["systems"][b_key]["delivery"][b]["mean"]
    d = a - h
    score = 5 if d > 0.20 else 4 if d > 0.05 else 3 if d >= -0.05 else 2 if d >= -0.20 else 1
    return {"scored": True, "A_regent": a, "B_hermes": h, "difference": d,
            "score": score, "budget": spec.PRIMARY_BUDGET, "arm": "raised"}


def report(r):
    b = str(spec.PRIMARY_BUDGET)
    print(f"\n== target_delivery@{b} ==")
    for k, v in r["systems"].items():
        print(f"  {k:28s} all {v['delivery'][b]['mean']:.3f}  "
              f"L {v['delivery_L'][b]['mean']:.3f}  D {v['delivery_D'][b]['mean']:.3f}  "
              f"mrr {v['mrr']['mean']:.3f}")
    print(f"\n== baselines @{b} (shared renderer) ==")
    for k, v in sorted(r["baselines"].items(), key=lambda p: -p[1]["delivery"][b]["mean"]):
        print(f"  {k:24s} all {v['delivery'][b]['mean']:.3f}  "
              f"L {v['delivery_L'][b]['mean']:.3f}  D {v['delivery_D'][b]['mean']:.3f}  "
              f"mrr {v['mrr']['mean']:.3f}")
    print(f"\n== score (§6.2) == {json.dumps(r['score'])}")


if __name__ == "__main__":
    main()

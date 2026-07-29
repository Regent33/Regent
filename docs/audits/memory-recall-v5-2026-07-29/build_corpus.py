#!/usr/bin/env python3
"""Build the v5 corpora, queries, targets and insertion orders.

Everything here is determined by protocol v5 (`../memory-recall-protocol-v5-2026-07-29.md`,
frozen `c038286`, corrected `8149415`). The builder asserts the protocol's own
claims instead of trusting the prose — v3 shipped an assertion that counted its
own labels and would have passed `{"topic": "auth", "text": "bananas are yellow"}`.

    python build_corpus.py <outdir>
"""

import itertools
import json
import pathlib
import random
import sys

import spec

OUT = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else ".")
OUT.mkdir(parents=True, exist_ok=True)

# --- §2.2 the templates must not leak the marker they are meant to withhold ---
marker_tokens = {
    t for m in spec.MAPS.values() for v in m.values() for t in spec.toks(v)
}
for q in spec.D_TEMPLATES:
    leaked = spec.toks(q.format(entity="X", relation="Y")) & marker_tokens
    assert not leaked, f"stratum D template leaks {leaked}: {q}"
    assert "current" not in spec.toks(q.format(entity="X", relation="Y"))
for q in spec.L_TEMPLATES:
    assert "current" in spec.toks(q.format(entity="X", relation="Y")), q

rng = random.Random(spec.BUILD_SEED)
corpus, queries, targets = [], [], {}
gold_meta = []

# --- 20 gold + 60 slot-confusable negatives (§2.1-§2.3) ---
for i, (entity, relation, values) in enumerate(spec.GOLD_TUPLES):
    stratum = "L" if i < spec.N_PER_STRATUM else "D"
    mapping = spec.MAPS[spec.MAP_ASSIGNMENT[i]]
    tmpl = (spec.L_TEMPLATES if stratum == "L" else spec.D_TEMPLATES)[i % 4]

    gid = f"g{i:02d}"
    corpus.append(spec.entry(gid, stratum, mapping, "gold", entity, relation,
                             values["current"], spec.DATES["current"]))
    # same entity, same relation, different value, superseded / rejected marker
    for kind in ("superseded", "rejected"):
        corpus.append(spec.entry(f"n{i:02d}{kind[0]}", stratum, mapping, kind,
                                 entity, relation, values[kind],
                                 spec.DATES[kind]))
    # different entity, same relation, different value, gold marker
    other = spec.OTHER_ENTITIES[i]
    corpus.append(spec.entry(f"n{i:02d}o", stratum, mapping, "other_entity",
                             other, relation, values["other"],
                             spec.DATES["current"]))

    queries.append({"id": f"q{i:02d}", "stratum": stratum, "template": i % 4,
                    "map": spec.MAP_ASSIGNMENT[i],
                    "text": tmpl.format(entity=entity, relation=relation),
                    "gold": [gid]})
    targets[f"q{i:02d}"] = gid
    gold_meta.append({"id": gid, "entity": entity, "relation": relation,
                      "stratum": stratum, "map": spec.MAP_ASSIGNMENT[i]})

# --- 420 filler, same templates, unrelated tuples, matched status prevalence ---
filler_pool = list(itertools.product(spec.FILLER_ENTITIES, spec.FILLER_RELATIONS))
rng.shuffle(filler_pool)
assert len(filler_pool) >= spec.N_FILLER, len(filler_pool)
for j in range(spec.N_FILLER):
    entity, relation = filler_pool[j]
    stratum = "L" if j % 2 == 0 else "D"
    mapping = spec.MAPS[spec.MAP_ASSIGNMENT[j % len(spec.MAP_ASSIGNMENT)]]
    kind = ("gold", "superseded", "rejected")[j % 3]  # marker prevalence 1:1:1
    corpus.append(spec.entry(f"f{j:03d}", stratum, mapping, kind, entity,
                             relation, spec.FILLER_VALUES[j % len(spec.FILLER_VALUES)],
                             spec.DATES["current" if kind == "gold" else kind]))

# --- §2.3 / §8 assertions, all fatal ---
assert len(corpus) == spec.N_TOTAL, len(corpus)
texts = [e["text"] for e in corpus]
assert len(set(texts)) == len(texts), "corpus text is not globally unique"
ids = [e["id"] for e in corpus]
assert len(set(ids)) == len(ids), "duplicate corpus id"
assert len(targets) == len(set(targets.values())) == spec.N_GOLD, "targets not a bijection"
for e in corpus:
    for other_id in ids:
        assert other_id not in e["text"], f"corpus id {other_id} leaked into {e['id']}"
# every negative differs from its gold in exactly the slots the protocol claims
by_id = {e["id"]: e for e in corpus}
for i in range(spec.N_GOLD):
    g = by_id[f"g{i:02d}"]
    for suffix, same_entity, same_status in (("s", True, False), ("r", True, False),
                                             ("o", False, True)):
        n = by_id[f"n{i:02d}{suffix}"]
        assert (n["entity"] == g["entity"]) is same_entity, n["id"]
        assert n["relation"] == g["relation"], n["id"]
        assert n["value"] != g["value"], n["id"]
        assert (n["marker"] == g["marker"]) is same_status, n["id"]

# --- §2.5 intervention corpus: derange gold-linked entities across the 60 ---
neg_ids = [e["id"] for e in corpus if e["id"].startswith("n")]
perm = spec.derangement(rng, spec.N_GOLD)
intervention = []
for e in corpus:
    if e["id"] not in neg_ids:
        intervention.append(dict(e))
        continue
    i = int(e["id"][1:3])
    swap = spec.GOLD_TUPLES[perm[i]][0] if e["kind"] != "other_entity" \
        else spec.OTHER_ENTITIES[perm[i]]
    intervention.append(spec.entry(e["id"], e["stratum"],
                                   spec.MAPS[spec.MAP_ASSIGNMENT[i]], e["kind"],
                                   swap, e["relation"], e["value"], e["date"]))
assert [e["id"] for e in intervention] == ids, "intervention changed ids or order"
marginals = spec.report_marginals(corpus, intervention)
assert marginals["entity_unigrams_preserved"], marginals
assert marginals["token_length_within_2"], marginals   # §2.5 froze +/-2 tokens

# --- 12 insertion orders (§2.4) ---
for seed in spec.SEEDS:
    r = random.Random(seed)
    order = ids[:]
    r.shuffle(order)
    gold_pos = sorted(order.index(f"g{i:02d}") for i in range(spec.N_GOLD))
    assert gold_pos[0] < spec.N_TOTAL // 4 and gold_pos[-1] > 3 * spec.N_TOTAL // 4, \
        f"gold not spread across corpus at seed {seed}: {gold_pos}"
    (OUT / f"order-seed{seed}.json").write_text(json.dumps(order), encoding="utf-8")

for name, obj in (("corpus.json", corpus), ("corpus-intervention.json", intervention),
                  ("queries.json", queries), ("targets.json", targets),
                  ("gold-meta.json", gold_meta), ("marginals.json", marginals)):
    (OUT / name).write_text(json.dumps(obj, indent=1) + "\n", encoding="utf-8")

print(f"corpus {len(corpus)} ({spec.N_GOLD} gold / {len(neg_ids)} confusable / "
      f"{spec.N_FILLER} filler), {len(queries)} queries, {len(spec.SEEDS)} orders")
print("marginals preserved:", {k: v for k, v in marginals.items() if k.endswith("preserved")})

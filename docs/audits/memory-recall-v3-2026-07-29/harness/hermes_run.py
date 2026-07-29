#!/usr/bin/env python3
"""Hermes's half of the v3 measurement.

Drives `MemoryStore` directly — the same class the memory tool uses — so the
corpus goes in through Hermes's own write path, budget and injection scan
included. No LLM: Hermes's built-in memory has no retrieval, it injects the
whole block. That is the finding, not a limitation of the harness.

The cap is passed to the constructor, which is Hermes's own public API:

    def __init__(self, memory_char_limit: int = 2200, user_char_limit: int = 1375)

so the `raised` arm is configuration, not a patch.

Isolated to a scratch HERMES_HOME, asserted before writing anything.

  v3_hermes.py <hermes-src> <artifacts> <seed> <cap> <arm> <out.json>
"""

import json
import os
import pathlib
import shutil
import sys

src, artifacts, seed, cap, arm, out = (
    sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4]), sys.argv[5], sys.argv[6])

home = pathlib.Path(out).parent / f"hermes-home-{arm}-s{seed}"
if home.exists():
    shutil.rmtree(home)
home.mkdir(parents=True)
os.environ["HERMES_HOME"] = str(home)

sys.path.insert(0, src)
from tools.memory_tool import MemoryStore, get_memory_dir  # noqa: E402

resolved = get_memory_dir()
assert str(home) in str(resolved), f"refusing to run: memories resolved to {resolved}"

A = pathlib.Path(artifacts)
corpus = json.loads((A / "corpus.json").read_text(encoding="utf-8"))
queries = json.loads((A / "queries.json").read_text(encoding="utf-8"))
order = json.loads((A / f"order-seed{seed}.json").read_text(encoding="utf-8"))
text_of = {e["id"]: e["text"] for e in corpus}
id_of = {e["text"]: e["id"] for e in corpus}

store = MemoryStore(memory_char_limit=cap, user_char_limit=cap)
store.load_from_disk()

stored, refused = [], []
for cid in order:
    result = store.add("memory", text_of[cid])
    (stored if result.get("success") else refused).append(cid)

# What actually reaches the model: the whole block, every turn, same for every
# query. Re-load so this is what a fresh session would see on disk.
store.load_from_disk()
block = store.format_for_system_prompt("memory") or ""
entries = list(store._entries_for("memory"))
delivered_order = [id_of.get(e, f"?{e[:24]}") for e in entries]

# Protocol §3.1: truncate the natural delivery to a token budget, in the
# system's own natural order. Hermes's natural order is block order.
BUDGETS = [250, 500, 1000, 2000, 4000]


def prefix_within(budget_tokens):
    """Ids that fit in `budget_tokens`, taken in block order (~4 chars/token)."""
    limit = budget_tokens * 4
    used, out_ids = 0, []
    for cid in delivered_order:
        cost = len(text_of.get(cid, "")) + len("\n§\n")
        if used + cost > limit:
            break
        used += cost
        out_ids.append(cid)
    return out_ids


results = []
for q in queries:
    results.append({
        "id": q["id"],
        "gold": q["gold"],
        # Identical for every query: Hermes cannot rank. Recorded per query
        # anyway so the scorer treats both systems the same way and the
        # asymmetry lands in the numbers rather than in the harness.
        "delivered": {str(b): prefix_within(b) for b in BUDGETS},
        "full_order": delivered_order,
    })

pathlib.Path(out).write_text(json.dumps({
    "system": "hermes", "arm": arm, "seed": seed, "cap": cap,
    "stored": stored, "refused": refused,
    "block_chars": len(block), "budgets": BUDGETS, "queries": results,
}, indent=1) + "\n", encoding="utf-8")
print(f"hermes {arm} s{seed} cap={cap}: stored {len(stored)} refused {len(refused)} "
      f"block {len(block)} chars")

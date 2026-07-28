#!/usr/bin/env python3
"""Hermes's half of the paired memory-recall pilot.

Drives `MemoryStore` directly — the same class the memory tool uses — so the
corpus goes in through Hermes's own write path, budget, injection scan and all.
No LLM: Hermes's built-in memory has no retrieval to speak of, it injects the
whole block, which is the finding rather than a limitation of the harness.

Isolated to a scratch HERMES_HOME. It asserts that before writing anything, so
the owner's real memories are never touched.

  hermes_run.py <hermes-src> <artifacts-dir> <N> <out.json>
"""

import json
import os
import pathlib
import shutil
import sys
import time

src, artifacts, n, out = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4]

# Isolate BEFORE importing anything that resolves the home.
home = pathlib.Path(out).parent / f"hermes-home-n{n}"
if home.exists():
    shutil.rmtree(home)
home.mkdir(parents=True)
os.environ["HERMES_HOME"] = str(home)

sys.path.insert(0, src)
from tools.memory_tool import MemoryStore, get_memory_dir  # noqa: E402

resolved = get_memory_dir()
assert str(home) in str(resolved), f"refusing to run: memories resolved to {resolved}"
print(f"isolated HERMES_HOME -> {resolved}")

corpus = json.loads(pathlib.Path(f"{artifacts}/corpus.json").read_text(encoding="utf-8"))
queries = json.loads(pathlib.Path(f"{artifacts}/queries.json").read_text(encoding="utf-8"))

store = MemoryStore()
store.load_from_disk()

stored, refused = [], []
by_text = {}
for entry in corpus[:n]:
    by_text[entry["text"]] = entry["id"]
    result = store.add("memory", entry["text"])
    (stored if result.get("success") else refused).append(entry["id"])

# What actually reaches the model: the whole block, every turn, for every query.
# `format_for_system_prompt` reads the load-time snapshot, so re-load to capture
# what a fresh session would see with this corpus on disk.
store.load_from_disk()
block = store.format_for_system_prompt("memory") or ""

# Hermes's built-in has no ranking: the same entries reach the model regardless
# of the query. Recorded per query anyway so the scorer treats both systems
# identically and the asymmetry shows up in the numbers, not in the harness.
entries = [e for e in store._entries_for("memory")]
returned = [by_text.get(e, f"?{e[:20]}") for e in entries]

results = []
for q in queries:
    start = time.perf_counter()
    _ = store.format_for_system_prompt("memory")
    ms = (time.perf_counter() - start) * 1000.0
    results.append(
        {
            "id": q["id"],
            "kind": q["kind"],
            "gold": q["gold"],
            "returned": returned,
            "ms": ms,
        }
    )

pathlib.Path(out).write_text(
    json.dumps(
        {
            "system": "hermes",
            "n": n,
            "stored": stored,
            "refused": refused,
            "block_chars": len(block),
            "queries": results,
        },
        indent=1,
    )
    + "\n",
    encoding="utf-8",
)
print(f"N={n}: stored {len(stored)} refused {len(refused)} | block {len(block)} chars")
print(f"wrote {out}")

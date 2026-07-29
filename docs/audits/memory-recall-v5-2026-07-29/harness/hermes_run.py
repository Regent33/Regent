#!/usr/bin/env python3
"""Hermes's half of the v5 measurement.

Drives `MemoryStore` directly — the class the memory tool uses — so the corpus
goes in through Hermes's own write path, budget and injection scan included.
No LLM: Hermes's built-in memory has no retrieval, it injects the whole block.
That is the finding, not a limitation of the harness.

Protocol v5 §5: this emits PARTS ONLY — the ordered ids it would deliver, the
rendered text of each, and Hermes's own join template. It performs no
tokenization and no truncation; the scorer does both, for both systems, in one
place. v3 split that rule across two files and made it unauditable.

    hermes_run.py <hermes-src> <artifacts> <seed> <arm> <corpus-file> <out.json>
"""

import json
import os
import pathlib
import shutil
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))
import spec  # noqa: E402

src, artifacts, seed, arm, corpus_file, out = sys.argv[1:7]
seed = int(seed)

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
corpus = json.loads((A / corpus_file).read_text(encoding="utf-8"))
queries = json.loads((A / "queries.json").read_text(encoding="utf-8"))
order = json.loads((A / f"order-seed{seed}.json").read_text(encoding="utf-8"))
text_of = {e["id"]: e["text"] for e in corpus}
id_of = {e["text"]: e["id"] for e in corpus}

# §3.1 shipped = MemoryStore() with NO arguments. §3.2 raised = 200_000 via the
# same public constructor. Neither is a patch; both are configuration.
store = (MemoryStore() if arm == "shipped"
         else MemoryStore(memory_char_limit=spec.RAISED_CHARS,
                          user_char_limit=spec.RAISED_CHARS))
store.load_from_disk()

stored, refused = [], []
for cid in order:
    result = store.add("memory", text_of[cid])
    (stored if result.get("success") else refused).append(cid)

# §3.2 — capacity must be neutralised in the raised arm or delivery is not what
# is being compared. Fatal, deliberately.
if arm == "raised":
    assert not refused, (
        f"raised arm refused {len(refused)} of {len(order)} entries; "
        "capacity is not neutralised, aborting per protocol §3.2")

# Re-load so this is what a fresh session would see on disk.
store.load_from_disk()
block = store.format_for_system_prompt("memory") or ""
entries = list(store._entries_for("memory"))
delivered_order = [id_of.get(e, f"?{e[:24]}") for e in entries]
assert not any(i.startswith("?") for i in delivered_order), \
    "unknown id in Hermes delivery — hard error per protocol §8"
assert len(set(delivered_order)) == len(delivered_order), "id delivered twice"
assert not (set(delivered_order) & set(refused)), "a refused id was delivered"

# Hermes's own rendering, read off the product rather than assumed. The block is
# a rule line, a title line carrying a live char counter, another rule line, and
# then `ENTRY_DELIMITER.join(entries)`. Guessing "\n\n" here is what the
# assertion below caught, and that header is real budget the model pays for.
from tools.memory_tool import ENTRY_DELIMITER  # noqa: E402

body = ENTRY_DELIMITER.join(text_of[i] for i in delivered_order)
assert block.endswith(body), (
    "Hermes's block does not end with the delimiter-join of its entries; the "
    "renderer template is wrong and every token count would be too")
template = {"prefix": block[:len(block) - len(body)],
            "separator": ENTRY_DELIMITER, "suffix": "",
            "block_chars": len(block)}

pathlib.Path(out).write_text(json.dumps({
    "system": "hermes", "arm": arm, "seed": seed, "corpus": corpus_file,
    "stored": stored, "refused": refused, "template": template,
    # Identical for every query: Hermes cannot rank. Recorded per query anyway so
    # the scorer treats both systems the same way and the asymmetry lands in the
    # numbers rather than in the harness.
    "queries": [{"id": q["id"], "gold": q["gold"], "ranked": delivered_order,
                 "rendered": [text_of[i] for i in delivered_order]}
                for q in queries],
    "raw": {i: text_of[i] for i in delivered_order},
}, indent=1) + "\n", encoding="utf-8")
print(f"hermes {arm} s{seed} {corpus_file}: stored {len(stored)} refused "
      f"{len(refused)} block {len(block)} chars header {len(template['prefix'])} chars")

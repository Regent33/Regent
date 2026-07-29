#!/usr/bin/env python3
"""Paired process-cost harness. Protocol: ../process-cost-protocol-2026-07-29.md

L1 — product readiness, as separate OS processes.
L2 — identical memory work, each in its native stack.

Interleaved, 11 reps, first discarded. Asserts both arms actually stored the
corpus before any timing counts: an arm that silently refused writes looks fast.

    python bench.py <hermes-src> <regent-l2-exe> <deacon-exe> <out.json>
"""

import json
import pathlib
import shutil
import statistics
import subprocess
import sys
import tempfile
import time

HERMES_SRC, L2_EXE, DEACON, OUT = sys.argv[1:5]
REPS, N, RAISED = 11, 100, 200_000
RECORD = "benchmark record {:03d} - fixed width padding to sixty chars.xx"
assert len(RECORD.format(0)) == 60, len(RECORD.format(0))


def timed(fn):
    t0 = time.perf_counter()
    r = fn()
    return (time.perf_counter() - t0) * 1000.0, r


def peak_rss(proc_args, stdin_bytes=None, ready_check=None):
    """Run a child to readiness, return (ms, peak_working_set_bytes)."""
    import psutil
    t0 = time.perf_counter()
    p = subprocess.Popen(proc_args, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                         stderr=subprocess.DEVNULL)
    peak = 0
    try:
        ps = psutil.Process(p.pid)
        if stdin_bytes:
            p.stdin.write(stdin_bytes)
            p.stdin.flush()
        while True:
            try:
                peak = max(peak, ps.memory_info().rss)
            except Exception:
                pass
            if ready_check is None:
                if p.poll() is not None:
                    break
            else:
                line = p.stdout.readline()
                if line and ready_check(line):
                    break
                if not line and p.poll() is not None:
                    break
    finally:
        ms = (time.perf_counter() - t0) * 1000.0
        try:
            p.kill()
        except Exception:
            pass
    return ms, peak


# ---------------------------------------------------------------- L1 readiness
def l1_regent():
    req = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "ping"}) + "\n"
    return peak_rss([DEACON], req.encode(), lambda b: b'"id"' in b or b"result" in b)


def l1_hermes():
    code = (f"import sys; sys.path.insert(0, r'{HERMES_SRC}');"
            "import cli; print('READY', flush=True)")
    return peak_rss([sys.executable, "-c", code], None, lambda b: b.startswith(b"READY"))


# ------------------------------------------------------- L2 same memory work
def l2_hermes(tmp):
    code = f'''
import json, os, pathlib, sys, time
home = pathlib.Path(r"{tmp}"); home.mkdir(parents=True, exist_ok=True)
os.environ["HERMES_HOME"] = str(home)
sys.path.insert(0, r"{HERMES_SRC}")
from tools.memory_tool import MemoryStore
t0 = time.perf_counter()
s = MemoryStore(memory_char_limit={RAISED}, user_char_limit={RAISED})
s.load_from_disk()
open_ms = (time.perf_counter() - t0) * 1000
t0 = time.perf_counter()
ok = 0
for i in range({N}):
    if s.add("memory", "{RECORD}".format(i)).get("success"): ok += 1
write_ms = (time.perf_counter() - t0) * 1000
t0 = time.perf_counter()
s.load_from_disk(); block = s.format_for_system_prompt("memory") or ""
render_ms = (time.perf_counter() - t0) * 1000
print(json.dumps({{"open_ms": open_ms, "write_ms": write_ms,
                  "render_ms": render_ms, "stored": ok, "block": len(block)}}))
'''
    r = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(r.stderr[-2000:])
    return json.loads(r.stdout.strip().splitlines()[-1])


def l2_regent(tmp):
    r = subprocess.run([L2_EXE, tmp, str(N), str(RAISED)], capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(r.stderr[-2000:])
    return json.loads(r.stdout.strip().splitlines()[-1])


def stats(xs):
    return {"median": statistics.median(xs), "min": min(xs), "max": max(xs),
            "n": len(xs), "all": xs}


def main():
    scratch = pathlib.Path(tempfile.mkdtemp(prefix="proccost-"))
    cells = {k: [] for k in ("l1_regent_ms", "l1_regent_rss", "l1_hermes_ms",
                             "l1_hermes_rss")}
    l2 = {"regent": [], "hermes": []}

    for rep in range(REPS):
        # Interleaved, so machine drift hits both arms equally (protocol §3).
        ms, rss = l1_regent(); cells["l1_regent_ms"].append(ms); cells["l1_regent_rss"].append(rss)
        ms, rss = l1_hermes(); cells["l1_hermes_ms"].append(ms); cells["l1_hermes_rss"].append(rss)
        rd = scratch / f"r{rep}"; hd = scratch / f"h{rep}"
        l2["regent"].append(l2_regent(str(rd)))
        l2["hermes"].append(l2_hermes(str(hd)))
        print(f"rep {rep + 1}/{REPS}", flush=True)

    # §5.5 — an arm that silently refused writes would look fast. Fatal.
    for sysname, rows in l2.items():
        bad = [r for r in rows if r["stored"] != N]
        assert not bad, f"{sysname} stored {bad[0]['stored']} of {N}; timings void"

    drop = lambda xs: xs[1:]        # first rep is warm-up (§3)
    out = {"protocol": "process-cost-protocol-2026-07-29", "reps": REPS,
           "reps_scored": REPS - 1, "n_entries": N, "raised_chars": RAISED,
           "l1": {k: stats(drop(v)) for k, v in cells.items()}, "l2": {}}
    for sysname, rows in l2.items():
        rows = drop(rows)
        for field in ("open_ms", "write_ms", "render_ms"):
            out["l2"].setdefault(sysname, {})[field] = stats([r[field] for r in rows])
        out["l2"][sysname]["block_chars"] = rows[0]["block"]
        out["l2"][sysname]["total_ms"] = stats(
            [r["open_ms"] + r["write_ms"] + r["render_ms"] for r in rows])

    pathlib.Path(OUT).write_text(json.dumps(out, indent=1) + "\n", encoding="utf-8")
    shutil.rmtree(scratch, ignore_errors=True)
    report(out)


def verdict(a, b):
    """§4 — A = Regent, B = Hermes. Lower is better for every cell here."""
    if abs(a - b) <= 0.10 * max(a, b):
        return 3, "tie"
    if a < b:
        return (5, f"regent {b / a:.1f}x") if b / a > 2 else (4, f"regent {b / a:.1f}x")
    return (1, f"hermes {a / b:.1f}x") if a / b > 2 else (2, f"hermes {a / b:.1f}x")


def report(o):
    print("\n== L1 product readiness (sanity check, not an algorithm result) ==")
    rm, hm = o["l1"]["l1_regent_ms"]["median"], o["l1"]["l1_hermes_ms"]["median"]
    rr, hr = o["l1"]["l1_regent_rss"]["median"], o["l1"]["l1_hermes_rss"]["median"]
    print(f"  ready ms : regent {rm:8.1f}  hermes {hm:8.1f}   -> {verdict(rm, hm)}")
    print(f"  peak RSS : regent {rr / 1e6:8.1f}M hermes {hr / 1e6:8.1f}M  -> {verdict(rr, hr)}")
    print("\n== L2 identical memory work, native stacks ==")
    for f in ("open_ms", "write_ms", "render_ms", "total_ms"):
        a, b = o["l2"]["regent"][f]["median"], o["l2"]["hermes"][f]["median"]
        print(f"  {f:10s}: regent {a:8.3f}  hermes {b:8.3f}   -> {verdict(a, b)}")
    print(f"  block chars: regent {o['l2']['regent']['block_chars']}  "
          f"hermes {o['l2']['hermes']['block_chars']}")


if __name__ == "__main__":
    main()

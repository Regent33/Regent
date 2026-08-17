#!/usr/bin/env python3
"""The managed-key catalog is hand-copied into TypeScript. Prove the copy matches.

`regent keys` (CLI) must be able to set exactly what the deacon's key tool
accepts and the Desktop API Keys page displays — otherwise `regent keys set X`
succeeds while the app never shows X, or the app offers a row the CLI refuses.
Both halves are plain literal lists, so this parses them and compares.

It checks two things, because the first alone is not enough:

  1. NAMES  — the same set of variables on both sides.
  2. GROUPS — the same UI section for each variable. The Rust side computes the
     group from name substrings while TypeScript spells it out per row, so the
     two can agree on every name and still file five of them differently. They
     did: OLLAMA, AIMLAPI, AZURE_OPENAI, RUNPOD and REGENT_BROWSER_MCP_URL each
     landed in a different section depending on which surface you opened.

Run by the `parity` CI job on both OSes, beside verify-versions.py.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RUST = ROOT / "src/crates/regent-tools/src/infra/key_tool/catalog.rs"
TS = ROOT / "src/regent-cli/src/features/keys/domain/keyCatalog.ts"

# TypeScript spells groups for humans; the deacon emits the short tokens the
# Desktop page switches on. One mapping, declared once.
TS_TO_RUST_GROUP = {
    "AI models": "llm",
    "Local models": "local",
    "Search": "search",
    "Messaging": "messaging",
    "Speech": "speech",
    "Vision": "vision",
    "Image generation": "image",
    "Video generation": "video",
    "Integrations": "integrations",
}


def rust_const_list(source: str, name: str) -> list[str]:
    """The string literals of a `const NAME: &[&str] = &[...]` block."""
    body = source.split(f"const {name}: &[&str] = &[", 1)[1].split("];", 1)[0]
    return re.findall(r'"([^"]+)"', body)


def rust_catalog(source: str) -> dict[str, str]:
    """Every MANAGED var, mapped through a faithful port of `key_group`."""
    managed_body = source.split("pub const MANAGED", 1)[1].split("];", 1)[0]
    names = re.findall(r'\(\s*"([A-Z0-9_]+)"', managed_body)

    group_fn = source.split("pub fn key_group", 1)[1]
    image = rust_const_list(group_fn, "IMAGE")
    video = rust_const_list(group_fn, "VIDEO")
    local = rust_const_list(group_fn, "LOCAL")
    messaging = rust_const_list(group_fn, "MESSAGING")
    search = rust_const_list(group_fn, "SEARCH")
    speech = rust_const_list(group_fn, "SPEECH")
    # The exact-name arms, read from source rather than restated here — a
    # hard-coded copy would be a third place to drift.
    exact = dict(re.findall(r'if name == "([A-Z0-9_]+)" \{\s*return "([a-z]+)";', group_fn))

    def classify(name: str) -> str:
        if name in exact:
            return exact[name]
        # Order matters and mirrors the Rust exactly.
        for patterns, group in (
            (image, "image"),
            (video, "video"),
            (local, "local"),
            (messaging, "messaging"),
            (search, "search"),
            (speech, "speech"),
        ):
            if any(p in name for p in patterns):
                return group
        return "llm"

    return {name: classify(name) for name in names}


def ts_catalog(source: str) -> dict[str, str]:
    body = source.split("export const MANAGED_KEYS", 1)[1].split("\n];", 1)[0]
    groups = "|".join(re.escape(g) for g in TS_TO_RUST_GROUP)
    rows = re.findall(
        rf'row\(\s*"([A-Z0-9_]+)",(?:[^()]|\([^()]*\))*?"({groups})"\s*,?\s*\)',
        body,
        re.S,
    )
    return {name: TS_TO_RUST_GROUP[group] for name, group in rows}


def main() -> int:
    rust = rust_catalog(RUST.read_text(encoding="utf-8"))
    ts = ts_catalog(TS.read_text(encoding="utf-8"))

    problems: list[str] = []
    if not rust or not ts:
        problems.append(f"parsed nothing: rust={len(rust)} ts={len(ts)} - the parser needs updating")

    for name in sorted(set(ts) - set(rust)):
        problems.append(f"{name}: in the CLI catalog, missing from Rust - `regent keys set` would accept a variable the deacon rejects and the app never shows")
    for name in sorted(set(rust) - set(ts)):
        problems.append(f"{name}: in Rust, missing from the CLI catalog - the app offers a row `regent keys set` refuses")
    for name in sorted(set(rust) & set(ts)):
        if rust[name] != ts[name]:
            problems.append(f"{name}: group differs - app shows '{rust[name]}', CLI lists '{ts[name]}'")

    if problems:
        print(f"key-catalog parity FAILED ({len(problems)} problem(s)):", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        print(f"\n  Rust: {RUST.relative_to(ROOT)}\n  TS:   {TS.relative_to(ROOT)}", file=sys.stderr)
        return 1

    counts: dict[str, int] = {}
    for group in rust.values():
        counts[group] = counts.get(group, 0) + 1
    summary = ", ".join(f"{g}={n}" for g, n in sorted(counts.items()))
    print(f"key-catalog parity OK: {len(rust)} managed keys agree on name and group ({summary})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

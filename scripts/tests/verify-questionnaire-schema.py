#!/usr/bin/env python3
"""The questionnaire contract is hand-copied from Rust into two TypeScript files.

`regent-kernel` authors the wire shape; the CLI and the Desktop app each carry
their own copy because they are separate build roots that cannot import Rust or
each other. Three copies of one contract is exactly the drift this repo has
already been bitten by (see verify-key-catalog.py), so this proves they agree.

Three checks, because any one alone would miss a real break:

  1. TS-vs-TS  — the two TypeScript copies are identical below their header.
     A field added to one surface and not the other renders a card in the app
     that the CLI cannot answer.
  2. KINDS     — the `QuestionKind` variants and the `Answer` tags match the
     Rust serde names one-for-one. Rust renames to snake_case on the wire; a
     variant added in Rust and missed in TS is an unhandled arm at runtime.
  3. FIELDS    — every struct field name in the Rust contract appears in the TS
     copy. Serde emits field names verbatim, so a Rust-side rename silently
     drops the value on the TypeScript side.

Run by the `parity` CI job on both OSes, beside verify-key-catalog.py.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RUST = ROOT / "src/crates/regent-kernel/src/contracts/questionnaire.rs"
TS_CLI = ROOT / "src/regent-cli/src/features/chat/domain/questionnaire.ts"
TS_APP = ROOT / "src/regent-app/Desktop/shared/kernel/questionnaire.ts"

# Structs whose serde field names must survive the copy. Rust `Vec<(String,
# Answer)>` becomes a TS tuple array, so shapes differ — names must not.
STRUCTS = ("QuestionOption", "Question", "Questionnaire", "QuestionnaireAnswer")

failures: list[str] = []


def fail(message: str) -> None:
    failures.append(message)


def body_of(source: str, header: str) -> str:
    """The `{...}` block following `header`, brace-matched."""
    start = source.index(header) + len(header)
    start = source.index("{", start)
    depth = 0
    for i in range(start, len(source)):
        if source[i] == "{":
            depth += 1
        elif source[i] == "}":
            depth -= 1
            if depth == 0:
                return source[start + 1 : i]
    raise ValueError(f"unbalanced braces after {header!r}")


def snake(name: str) -> str:
    """Rust variant name → the serde snake_case wire name."""
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


def rust_variants(source: str, enum_name: str) -> list[str]:
    """Wire names of an enum's variants (serde rename_all = snake_case)."""
    body = body_of(source, f"pub enum {enum_name}")
    body = re.sub(r"//[^\n]*", "", body)
    return [snake(v) for v in re.findall(r"^\s*([A-Z]\w*)", body, re.MULTILINE)]


def rust_fields(source: str, struct_name: str) -> list[str]:
    body = body_of(source, f"pub struct {struct_name}")
    body = re.sub(r"//[^\n]*", "", body)
    return re.findall(r"^\s*pub (\w+):", body, re.MULTILINE)


def strip_header(text: str) -> str:
    """Everything after the leading `//` comment block, newlines normalized."""
    lines = text.replace("\r\n", "\n").split("\n")
    i = 0
    while i < len(lines) and (lines[i].startswith("//") or lines[i].strip() == ""):
        i += 1
    return "\n".join(lines[i:]).rstrip() + "\n"


def main() -> int:
    for path in (RUST, TS_CLI, TS_APP):
        if not path.is_file():
            print(f"MISSING: {path.relative_to(ROOT)}", file=sys.stderr)
            return 1

    rust = RUST.read_text(encoding="utf-8")
    cli = TS_CLI.read_text(encoding="utf-8")
    app = TS_APP.read_text(encoding="utf-8")

    # 1. The two TypeScript copies must be the same contract.
    if strip_header(cli) != strip_header(app):
        fail(
            "the two TypeScript copies have diverged:\n"
            f"  {TS_CLI.relative_to(ROOT)}\n"
            f"  {TS_APP.relative_to(ROOT)}\n"
            "  copy one over the other (below the header comment) and re-run."
        )

    # 2. Enum variants — the wire tags every surface switches on.
    for enum_name, ts_type in (("QuestionKind", "QuestionKind"), ("Answer", "Answer")):
        expected = rust_variants(rust, enum_name)
        ts_block = cli.split(f"export type {ts_type} =", 1)[1].split(";\n", 1)[0]
        found = set(re.findall(r'"([a-z_]+)"', ts_block))
        missing = [v for v in expected if v not in found]
        extra = [v for v in found if v not in expected]
        if missing or extra:
            fail(
                f"{enum_name}: TypeScript is out of sync with the Rust — "
                f"missing {missing or '[]'}, unexpected {extra or '[]'}"
            )

    # 3. Struct field names — serde emits them verbatim.
    for struct_name in STRUCTS:
        expected = rust_fields(rust, struct_name)
        try:
            ts_block = body_of(cli, f"export interface {struct_name}")
        except ValueError:
            fail(f"{struct_name}: no matching TypeScript interface")
            continue
        found = set(re.findall(r"^\s*readonly (\w+)", ts_block, re.MULTILINE))
        missing = [f for f in expected if f not in found]
        if missing:
            fail(f"{struct_name}: TypeScript is missing field(s) {missing}")

    if failures:
        print("Questionnaire schema parity FAILED\n", file=sys.stderr)
        for message in failures:
            print(f"  - {message}", file=sys.stderr)
        return 1

    print(
        f"Questionnaire schema parity OK — {len(STRUCTS)} structs, 2 enums, "
        "2 TypeScript copies in sync with the Rust contract."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

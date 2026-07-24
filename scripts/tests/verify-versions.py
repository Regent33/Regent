#!/usr/bin/env python3
"""Version + protocol parity gate (ADR-041 / cross-component-update Phase 0).

The workspace `Cargo.toml` version and the Rust voice-server `CALL_PROTOCOL`
are the single sources of truth. Every hand-copied version/protocol constant in
the CLI, Desktop, Installer, and Tauri surfaces must equal them. This exercises
the *canonical* value against each copy — no hard-coded "expected" literal — so
it keeps working across releases. Runs on Linux and Windows in CI.

Usage: python verify-versions.py [repo_root]   (defaults to the repo root)
Exit 0 = all aligned; exit 1 = drift, one line per mismatch.
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]


def _read(root: Path, rel: str) -> str:
    return (root / rel).read_text(encoding="utf-8")


def _toml_section_value(text: str, section: str, key: str) -> str:
    """First `key = "..."` after a `[section]` header, before the next header."""
    in_section = False
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("[") and stripped.endswith("]"):
            in_section = stripped == f"[{section}]"
            continue
        if in_section:
            m = re.match(rf'{re.escape(key)}\s*=\s*"([^"]+)"', stripped)
            if m:
                return m.group(1)
    raise ValueError(f"{key} not found under [{section}]")


def _re_group(text: str, pattern: str) -> str:
    m = re.search(pattern, text)
    if not m:
        raise ValueError(f"pattern not found: {pattern}")
    return m.group(1)


def workspace_version(root: Path = REPO_ROOT) -> str:
    return _toml_section_value(_read(root, "Cargo.toml"), "workspace.package", "version")


def voice_call_protocol(root: Path = REPO_ROOT) -> int:
    text = _read(root, "src/crates/regent-voice-server/src/infra/http/pages.rs")
    return int(_re_group(text, r"CALL_PROTOCOL\s*:\s*u32\s*=\s*(\d+)"))


# (label, relative path, extractor) — extractor returns the string found in file.
def _json_version(root: Path, rel: str) -> str:
    return str(json.loads(_read(root, rel))["version"])


def _brand_version(root: Path, rel: str) -> str:
    return _re_group(_read(root, rel), r'version:\s*"([^"]+)"')


def _tauri_cargo_version(root: Path, rel: str) -> str:
    return _toml_section_value(_read(root, rel), "package", "version")


VERSION_SURFACES = [
    ("cli package.json", "src/regent-cli/package.json", _json_version),
    ("cli brand.ts", "src/regent-cli/src/app/config/brand.ts", _brand_version),
    ("desktop package.json", "src/regent-app/Desktop/package.json", _json_version),
    ("desktop tauri.conf.json", "src/regent-app/Desktop/src-tauri/tauri.conf.json", _json_version),
    ("desktop src-tauri/Cargo.toml", "src/regent-app/Desktop/src-tauri/Cargo.toml", _tauri_cargo_version),
    ("installer package.json", "src/regent-app/Installer/package.json", _json_version),
    ("installer tauri.conf.json", "src/regent-app/Installer/src-tauri/tauri.conf.json", _json_version),
    ("installer src-tauri/Cargo.toml", "src/regent-app/Installer/src-tauri/Cargo.toml", _tauri_cargo_version),
]


def _proto_const(root: Path, rel: str) -> int:
    return int(_re_group(_read(root, rel), r"CALL_PROTOCOL\s*=\s*(\d+)"))


PROTOCOL_SURFACES = [
    ("cli voiceServe.ts", "src/regent-cli/src/features/voice/cli/voiceServe.ts"),
    ("desktop protocol.ts", "src/regent-app/Desktop/shared/infrastructure/voice/protocol.ts"),
]


def check(root: Path = REPO_ROOT) -> list[str]:
    """Return one human-readable line per drift; empty list means aligned."""
    problems: list[str] = []
    want_ver = workspace_version(root)
    for label, rel, extract in VERSION_SURFACES:
        try:
            got = extract(root, rel)
        except (OSError, ValueError, KeyError) as exc:
            problems.append(f"version: {label}: unreadable ({exc})")
            continue
        if got != want_ver:
            problems.append(f"version: {label} is {got!r}, workspace is {want_ver!r}")

    want_proto = voice_call_protocol(root)
    for label, rel in PROTOCOL_SURFACES:
        try:
            got = _proto_const(root, rel)
        except (OSError, ValueError) as exc:
            problems.append(f"call_protocol: {label}: unreadable ({exc})")
            continue
        if got != want_proto:
            problems.append(f"call_protocol: {label} is {got}, voice server is {want_proto}")
    return problems


def main() -> int:
    root = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else REPO_ROOT
    problems = check(root)
    if problems:
        print("version/protocol drift detected:")
        for line in problems:
            print(f"  - {line}")
        return 1
    print(
        f"OK: version {workspace_version(root)} and call protocol "
        f"{voice_call_protocol(root)} aligned across all surfaces"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

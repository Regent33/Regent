#!/usr/bin/env python3
"""Build the canonical release manifest (ADR-041 / update-system Phase 0).

Run once, as a same-run fan-in, *after* every platform archive and its
`.sha256` sidecar exist as this run's artifacts. The manifest is compact and
additive: component/protocol/schema facts plus each platform asset's name,
size, and SHA-256 — and never a download URL. Clients derive the official
GitHub release URL and re-check names against their own allowlist.

Canonical facts are read straight from source (workspace version, voice-server
call protocol, store/prompt/config schema constants) so the manifest cannot
drift from the code it describes. Phase 0 is unsigned: `signing_key_id` is null,
declaring the signing key absent.

Usage: python make-manifest.py <assets_dir> <output.json> [repo_root]
"""
from __future__ import annotations

import hashlib
import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]

# Strict asset allowlist: regent-<os>-<arch>.<ext>. Anything else is rejected.
ASSET_RE = re.compile(r"^regent-(windows|macos|linux)-(x86_64|aarch64)\.(zip|tar\.gz)$")

# Protocol facts without a single source constant in the tree today. Authored
# once here (no second copy to drift against); bump alongside the wire change.
DEACON_RPC = {"min": 1, "max": 1}
# Release policy floors (oldest peer still compatible / lowest safe rollback).
MINIMUM_SUPPORTED = "0.1.0"
ROLLBACK_MINIMUM_CORE = "0.1.1"


def _read(root: Path, rel: str) -> str:
    return (root / rel).read_text(encoding="utf-8")


def _re_group(text: str, pattern: str) -> str:
    m = re.search(pattern, text)
    if not m:
        raise ValueError(f"pattern not found: {pattern}")
    return m.group(1)


def workspace_version(root: Path) -> str:
    text = _read(root, "Cargo.toml")
    in_section = False
    for line in text.splitlines():
        s = line.strip()
        if s.startswith("[") and s.endswith("]"):
            in_section = s == "[workspace.package]"
            continue
        if in_section:
            m = re.match(r'version\s*=\s*"([^"]+)"', s)
            if m:
                return m.group(1)
    raise ValueError("workspace version not found")


def protocol_facts(root: Path) -> dict:
    call = int(_re_group(
        _read(root, "src/crates/regent-voice-server/src/infra/http/pages.rs"),
        r"CALL_PROTOCOL\s*:\s*u32\s*=\s*(\d+)",
    ))
    store = int(_re_group(
        _read(root, "src/crates/regent-store/src/infra/schema.rs"),
        r"SCHEMA_VERSION\s*:\s*i64\s*=\s*(\d+)",
    ))
    prompt = int(_re_group(
        _read(root, "src/crates/regent-agent/src/domain/prompts/system.rs"),
        r"regent-prompt-schema:v(\d+)",
    ))
    config = int(_re_group(
        _read(root, "src/crates/regent-deacon/src/domain/config/mod.rs"),
        r"CURRENT_CONFIG_VERSION\s*:\s*u32\s*=\s*(\d+)",
    ))
    return {
        "deacon_rpc": dict(DEACON_RPC),
        "call": {"min": call, "max": call},
        "prompt_schema": prompt,
        "store_schema": store,
        "config_schema": config,
    }


def _sidecar_hash(path: Path) -> str:
    # Sidecar line is "<sha256>  <name>" on both sha256sum and Get-FileHash.
    token = path.read_text(encoding="utf-8").strip().split()[0].lower()
    if not re.fullmatch(r"[0-9a-f]{64}", token):
        raise ValueError(f"{path.name}: not a valid sha256 sidecar")
    return token


def collect_assets(assets_dir: Path) -> dict:
    """Map platform key -> {name, sha256, size} from this run's archives.

    Rejects any non-sidecar file whose name is outside the allowlist, any asset
    missing its sidecar, and any sidecar whose hash disagrees with the archive.
    """
    assets: dict[str, dict] = {}
    for path in sorted(assets_dir.iterdir()):
        if not path.is_file() or path.name.endswith(".sha256"):
            continue
        m = ASSET_RE.match(path.name)
        if not m:
            raise ValueError(f"unexpected asset name (allowlist): {path.name}")
        sidecar = path.with_name(path.name + ".sha256")
        if not sidecar.exists():
            raise ValueError(f"missing sidecar for {path.name}")
        declared = _sidecar_hash(sidecar)
        with path.open("rb") as archive:
            actual = hashlib.file_digest(archive, "sha256").hexdigest()
        if declared != actual:
            raise ValueError(f"sha256 mismatch for {path.name}: sidecar {declared} vs file {actual}")
        key = f"{m.group(1)}-{m.group(2)}"
        assets[key] = {"name": path.name, "sha256": actual, "size": path.stat().st_size}
    if not assets:
        raise ValueError(f"no release archives found in {assets_dir}")
    return assets


def build_manifest(assets_dir: Path, root: Path = REPO_ROOT) -> dict:
    version = workspace_version(root)
    facts = protocol_facts(root)
    now = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    return {
        "schema": 1,
        "generated_at": now,
        "channels": {
            "stable": {
                "version": version,
                "released_at": now,
                "minimum_supported": MINIMUM_SUPPORTED,
                "protocols": facts,
                "components": {
                    "core": {
                        "version": version,
                        "contains": ["regent-cli", "regent-deacon"],
                        "assets": collect_assets(assets_dir),
                    },
                    "desktop-windows": {"version": version, "apply": "installer"},
                    "desktop-linux": {"version": version, "apply": "installer"},
                },
                "rollback": {
                    "minimum_core": ROLLBACK_MINIMUM_CORE,
                    "store_schema": facts["store_schema"],
                },
            }
        },
        "signing_key_id": None,
    }


def main(argv: list[str]) -> int:
    if len(argv) < 3:
        print("usage: make-manifest.py <assets_dir> <output.json> [repo_root]", file=sys.stderr)
        return 2
    root = Path(argv[3]).resolve() if len(argv) > 3 else REPO_ROOT
    manifest = build_manifest(Path(argv[1]).resolve(), root)
    Path(argv[2]).write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    stable = manifest["channels"]["stable"]
    print(f"wrote {argv[2]}: v{stable['version']} with {len(stable['components']['core']['assets'])} assets")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))

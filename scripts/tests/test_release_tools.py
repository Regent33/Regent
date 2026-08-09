#!/usr/bin/env python3
"""Unit tests for the release/version-truth tools (ADR-041 Phase 0).

Exercises the *actual* manifest generator and parity-check logic against local
fixtures — never source-text assertions. Stdlib only.

    python -m unittest discover -s scripts/tests -p 'test_*.py'
"""
from __future__ import annotations

import hashlib
import importlib.util
import json
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[1]


def _load(path: Path):
    spec = importlib.util.spec_from_file_location(path.stem.replace("-", "_"), path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


manifest = _load(REPO_ROOT / "scripts/release/make-manifest.py")
parity = _load(HERE / "verify-versions.py")


def _write(root: Path, rel: str, text: str) -> None:
    p = root / rel
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(text, encoding="utf-8")


def _asset(dir_: Path, name: str, body: bytes) -> None:
    (dir_ / name).write_bytes(body)
    digest = hashlib.sha256(body).hexdigest()
    (dir_ / (name + ".sha256")).write_text(f"{digest}  {name}\n", encoding="utf-8")


class ManifestGeneratorTests(unittest.TestCase):
    def _assets(self, dir_: Path) -> None:
        _asset(dir_, "regent-linux-x86_64.tar.gz", b"linux-archive-bytes")
        _asset(dir_, "regent-windows-x86_64.zip", b"windows-archive-bytes")

    def test_manifest_shape_facts_and_asset_hashes(self):
        with TemporaryDirectory() as td:
            adir = Path(td)
            self._assets(adir)
            m = manifest.build_manifest(adir, REPO_ROOT)

        self.assertEqual(m["schema"], 1)
        self.assertIsNone(m["signing_key_id"], "Phase 0 must declare signing key absent")
        stable = m["channels"]["stable"]
        # Version is derived from the real workspace Cargo.toml, not copied.
        self.assertEqual(stable["version"], parity.workspace_version(REPO_ROOT))
        # Protocol/schema facts come from source constants.
        proto = stable["protocols"]
        self.assertEqual(proto["call"], {"min": 7, "max": 7})
        self.assertEqual(proto["deacon_rpc"], {"min": 1, "max": 1})
        self.assertEqual(proto["store_schema"], 11)
        self.assertEqual(proto["prompt_schema"], 4)
        self.assertEqual(proto["config_schema"], 2)
        # Assets carry name/size/hash exactly matching this run's archives.
        assets = stable["components"]["core"]["assets"]
        self.assertEqual(set(assets), {"linux-x86_64", "windows-x86_64"})
        linux = assets["linux-x86_64"]
        self.assertEqual(linux["name"], "regent-linux-x86_64.tar.gz")
        self.assertEqual(linux["size"], len(b"linux-archive-bytes"))
        self.assertEqual(linux["sha256"], hashlib.sha256(b"linux-archive-bytes").hexdigest())
        # Desktop components are declared installer-applied, no hashes claimed.
        self.assertEqual(stable["components"]["desktop-linux"]["apply"], "installer")

    def test_manifest_contains_no_urls(self):
        with TemporaryDirectory() as td:
            adir = Path(td)
            self._assets(adir)
            blob = json.dumps(manifest.build_manifest(adir, REPO_ROOT)).lower()
        self.assertNotIn("http", blob)
        self.assertNotIn("://", blob)

    def test_rejects_asset_name_outside_allowlist(self):
        with TemporaryDirectory() as td:
            adir = Path(td)
            _asset(adir, "regent-linux-x86_64.tar.gz", b"ok")
            _asset(adir, "malware-linux-x86_64.tar.gz", b"nope")  # bad name
            with self.assertRaises(ValueError):
                manifest.build_manifest(adir, REPO_ROOT)

    def test_rejects_missing_sidecar(self):
        with TemporaryDirectory() as td:
            adir = Path(td)
            (adir / "regent-linux-x86_64.tar.gz").write_bytes(b"ok")  # no .sha256
            with self.assertRaises(ValueError):
                manifest.build_manifest(adir, REPO_ROOT)

    def test_rejects_hash_mismatch(self):
        with TemporaryDirectory() as td:
            adir = Path(td)
            (adir / "regent-linux-x86_64.tar.gz").write_bytes(b"real-bytes")
            (adir / "regent-linux-x86_64.tar.gz.sha256").write_text(
                "%s  regent-linux-x86_64.tar.gz\n" % ("0" * 64), encoding="utf-8"
            )
            with self.assertRaises(ValueError):
                manifest.build_manifest(adir, REPO_ROOT)

    def test_rejects_empty_asset_dir(self):
        with TemporaryDirectory() as td:
            with self.assertRaises(ValueError):
                manifest.build_manifest(Path(td), REPO_ROOT)


VER = "9.9.9"
PROTO = 7


def _fixture_tree(root: Path, *, version=VER, proto=PROTO, override=None) -> None:
    files = {
        "Cargo.toml": f'[workspace.package]\nversion = "{version}"\n',
        "src/regent-cli/package.json": json.dumps({"name": "regent-cli", "version": version}),
        "src/regent-cli/src/app/config/brand.ts": (
            f'export const BRAND = {{\n  name: "Regent",\n  version: "{version}",\n}};\n'
        ),
        "src/regent-app/Desktop/package.json": json.dumps({"version": version}),
        "src/regent-app/Desktop/src-tauri/tauri.conf.json": json.dumps({"version": version}),
        "src/regent-app/Desktop/src-tauri/Cargo.toml": f'[package]\nname = "d"\nversion = "{version}"\n',
        "src/regent-app/Installer/package.json": json.dumps({"version": version}),
        "src/regent-app/Installer/src-tauri/tauri.conf.json": json.dumps({"version": version}),
        "src/regent-app/Installer/src-tauri/Cargo.toml": f'[package]\nname = "i"\nversion = "{version}"\n',
        "src/crates/regent-voice-server/src/infra/http/pages.rs": (
            f"pub(super) const CALL_PROTOCOL: u32 = {proto};\n"
        ),
        "src/regent-cli/src/features/voice/cli/voiceServe.ts": (
            f"export const CALL_PROTOCOL = {proto};\n"
        ),
        "src/regent-app/Desktop/shared/infrastructure/voice/protocol.ts": (
            f"export const CALL_PROTOCOL = {proto};\n"
        ),
    }
    if override:
        files.update(override)
    for rel, text in files.items():
        _write(root, rel, text)


class ParityCheckTests(unittest.TestCase):
    def test_real_repo_is_aligned(self):
        # The production tree must pass its own gate.
        self.assertEqual(parity.check(REPO_ROOT), [])

    def test_aligned_fixture_has_no_problems(self):
        with TemporaryDirectory() as td:
            root = Path(td)
            _fixture_tree(root)
            self.assertEqual(parity.check(root), [])

    def test_detects_version_drift(self):
        with TemporaryDirectory() as td:
            root = Path(td)
            _fixture_tree(root, override={
                "src/regent-app/Desktop/package.json": json.dumps({"version": "0.0.0"}),
            })
            problems = parity.check(root)
            self.assertEqual(len(problems), 1, problems)
            self.assertIn("desktop package.json", problems[0])
            self.assertIn("0.0.0", problems[0])

    def test_detects_protocol_drift(self):
        with TemporaryDirectory() as td:
            root = Path(td)
            _fixture_tree(root, override={
                "src/regent-cli/src/features/voice/cli/voiceServe.ts":
                    "export const CALL_PROTOCOL = 4;\n",
            })
            problems = parity.check(root)
            self.assertEqual(len(problems), 1, problems)
            self.assertIn("cli voiceServe.ts", problems[0])


if __name__ == "__main__":
    unittest.main()

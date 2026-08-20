#!/usr/bin/env sh
# Stages everything "Regent Setup" ships inside itself (macOS / Linux).
#   sh src/regent-app/Installer/scripts/build-payload.sh
# Mirror of build-payload.ps1 — see it for the payload layout. Skip the slow
# parts with SKIP_CORE=1 / SKIP_APP=1 when iterating on one of them.
set -eu

installer="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$installer/../../.." && pwd)"
payload="$installer/src-tauri/payload"

case "$(uname -s)" in
  Darwin) os="macos" ;;
  Linux)  os="linux" ;;
  *) echo "unsupported OS: $(uname -s)"; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64|amd64)  arch="x86_64" ;;
  arm64|aarch64) arch="aarch64" ;;
  *) echo "unsupported arch: $(uname -m)"; exit 1 ;;
esac

mkdir -p "$payload/app"

# tauri.conf bundles payload/**/* wholesale, so whatever sits here is posted to
# users. Running the deacon with this as its cwd, for one, leaves an 87MB
# .fastembed_cache behind. Keep the directory to exactly what we stage.
find "$payload" -mindepth 1 -maxdepth 1 \
  ! -name 'README.md' ! -name 'regent-*' ! -name 'install.ps1' ! -name 'install.sh' ! -name 'app' \
  -exec sh -c 'echo "  pruning stray $(basename "$1")"; rm -rf "$1"' _ {} \;

if [ -z "${SKIP_CORE:-}" ]; then
  echo "==> deacon + gateway + CLI"
  # Same set the release asset carries. regent-gateway is what `regent gateway
  # start` spawns; regent-mcp needs no line of its own — it is `regent-deacon
  # mcp` now, a subcommand of the binary built here.
  (cd "$repo" && cargo build --release -p regent-deacon -p regent-gateway)
  (cd "$repo/src/regent-cli" && bun install --frozen-lockfile && bun run compile)

  # The mic and Butler Mode spawn this, and it was missing from the Linux
  # payload entirely — the Windows side was fixed first. Fatal on purpose: this
  # is a hand-run build, and a setup that silently cannot do voice is the bug
  # being fixed. Needs libclang for bindgen (apt install libclang-dev).
  (cd "$repo" && cargo build --release -p regent-voice-server) || {
    echo "regent-voice-server failed to build — install libclang-dev, then retry" >&2
    exit 1
  }

  # Archive layout must match the release asset: every binary at the root, so
  # the CLI finds its siblings after extraction.
  stage="$(mktemp -d)"
  trap 'rm -rf "$stage"' EXIT
  cp "$repo/target/release/regent-deacon" "$repo/src/regent-cli/dist/regent-cli" \
     "$repo/target/release/regent-voice-server" "$repo/target/release/regent-gateway" "$stage/"
  # The voice server loads sherpa-onnx/onnxruntime at runtime (dynamic by
  # necessity — sherpa's static feature rules out the prebuilt binaries, see
  # ADR-029). It finds them beside itself via the rpath its build.rs sets, but
  # only if they are actually there.
  cp "$repo"/target/release/*sherpa*.so* "$repo"/target/release/*onnxruntime*.so* "$stage/" 2>/dev/null || true
  ls "$stage"/*sherpa* >/dev/null 2>&1 || {
    echo "no sherpa runtime library in target/release — the voice server would not start" >&2
    exit 1
  }
  rm -f "$payload/regent-${os}-${arch}.tar.gz"
  tar -czf "$payload/regent-${os}-${arch}.tar.gz" -C "$stage" .
fi

if [ -z "${SKIP_APP:-}" ]; then
  echo "==> desktop app"
  # --no-bundle: we ship the bare binary and do our own placement; a .dmg
  # nested inside this installer would be pointless.
  (cd "$repo/src/regent-app/Desktop" && bun install --frozen-lockfile && bun run tauri build --no-bundle)
  cp "$repo/src/regent-app/Desktop/src-tauri/target/release/regent-desktop" "$payload/app/Regent"
  chmod +x "$payload/app/Regent"
fi

cp "$repo/scripts/install.sh" "$payload/"

echo
echo "payload ready: $payload"
find "$payload" -type f -exec ls -lh {} \; | awk '{printf "  %-28s %8s\n", $NF, $5}'

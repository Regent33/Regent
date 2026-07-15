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

if [ -z "${SKIP_CORE:-}" ]; then
  echo "==> deacon + CLI"
  (cd "$repo" && cargo build --release -p regent-deacon)
  (cd "$repo/src/regent-cli" && bun install --frozen-lockfile && bun run compile)

  # Archive layout must match the release asset: both binaries at the root, so
  # the CLI still finds regent-deacon as a sibling after extraction.
  stage="$(mktemp -d)"
  trap 'rm -rf "$stage"' EXIT
  cp "$repo/target/release/regent-deacon" "$repo/src/regent-cli/dist/regent-cli" "$stage/"
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

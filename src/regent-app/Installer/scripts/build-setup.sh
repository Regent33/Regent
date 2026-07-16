#!/usr/bin/env sh
# Builds "Regent Setup" for Linux (AppImage) / macOS (dmg) — the mirror of
# build-setup.ps1, which owns the Windows/NSIS side:
#   sh src/regent-app/Installer/scripts/build-setup.sh
#
# No code signing here: there is no Authenticode equivalent gating Linux
# downloads, and provenance comes from the Sigstore attestation CI attaches.
# (macOS notarization is a future problem — it needs an Apple Developer ID.)
# Skip the slow payload rebuild with SKIP_PAYLOAD=1 when iterating on Setup.
set -eu

installer="$(cd "$(dirname "$0")/.." && pwd)"
cd "$installer"

if [ -z "${SKIP_PAYLOAD:-}" ]; then
  sh scripts/build-payload.sh
fi

bun install --frozen-lockfile
bun run tauri build

echo
echo "built:"
find src-tauri/target/release/bundle -type f \
  \( -name '*.AppImage' -o -name '*.dmg' \) -exec ls -lh {} \;

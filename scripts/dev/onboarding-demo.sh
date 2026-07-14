#!/usr/bin/env sh
# See the Regent first-run onboarding in a sandboxed REGENT_HOME (your real
# ~/.regent is never touched):  sh scripts/dev/onboarding-demo.sh
set -eu
repo="$(cd "$(dirname "$0")/../.." && pwd)"
exe="$repo/src/regent-cli/dist/regent-cli"
[ -x "$exe" ] || { echo "dist binary missing — build it: cd src/regent-cli && bun run compile"; exit 1; }

demo_home="$(mktemp -d -t regent-onboarding-demo.XXXXXX)"
echo ""
echo "=== Regent onboarding demo (sandboxed) ==="
echo "REGENT_HOME = $demo_home  (your real ~/.regent is untouched)"
echo "Tip: pick 'ollama' to try the local no-key path. Ctrl+C exits chat."
echo ""
REGENT_HOME="$demo_home" "$exe" || true
echo ""
echo "Demo home left at $demo_home — inspect config.yaml/.env, delete when done:"
echo "  rm -rf $demo_home"

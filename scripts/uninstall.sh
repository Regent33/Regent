#!/usr/bin/env sh
# Regent uninstaller (macOS / Linux) — mirror image of install.sh:
#   curl -fsSL https://raw.githubusercontent.com/<owner>/<repo>/main/scripts/uninstall.sh | sh
# Stops Regent processes, removes ~/.regent/bin and the `regent` link.
# Your data in ~/.regent (config, keys, sessions, memory) is KEPT unless you
# pass --purge (or set REGENT_PURGE=1 when piping through sh).
# Idempotent: safe to run twice, or after a partial install.
set -eu

HOME_DIR="${REGENT_HOME:-$HOME/.regent}"
BIN_DIR="${REGENT_BIN_DIR:-$HOME_DIR/bin}"
LINK_DIR="${REGENT_LINK_DIR:-$HOME/.local/bin}"
PURGE="${REGENT_PURGE:-0}"
[ "${1:-}" = "--purge" ] && PURGE=1

# 1) Stop running Regent processes — pidfiles first, name match as fallback.
#    Works while things are mid-run; ignores what's already gone.
for pidfile in "$HOME_DIR"/*.pid; do
  [ -f "$pidfile" ] || continue
  pid="$(cat "$pidfile" 2>/dev/null || true)"
  [ -n "$pid" ] && kill "$pid" 2>/dev/null && echo "→ stopped pid $pid ($(basename "$pidfile"))" || true
  rm -f "$pidfile"
done
for name in regent-deacon regent-gateway regent-voice-server regent-cli; do
  pkill -x "$name" 2>/dev/null && echo "→ stopped $name" || true
done

# 2) Remove binaries + shim link.
if [ -d "$BIN_DIR" ]; then
  rm -rf "$BIN_DIR"
  echo "✓ removed $BIN_DIR"
fi
# Only remove the link if it is ours (points into BIN_DIR) or is dangling.
if [ -L "$LINK_DIR/regent" ]; then
  rm -f "$LINK_DIR/regent"
  echo "✓ removed $LINK_DIR/regent"
fi

# 3) Data: keep by default, delete on --purge (includes ~/.regent/src).
if [ "$PURGE" = "1" ]; then
  rm -rf "$HOME_DIR"
  echo "✓ purged $HOME_DIR (config, keys, sessions, memory, source checkout)"
else
  if [ -d "$HOME_DIR" ]; then
    echo "kept your data at $HOME_DIR (config, keys, sessions, memory)."
    echo "  to delete it too: uninstall.sh --purge   (or: rm -rf $HOME_DIR)"
  fi
fi

echo "✓ Regent uninstalled"

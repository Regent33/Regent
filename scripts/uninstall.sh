#!/usr/bin/env sh
# Regent uninstaller (macOS / Linux) — mirror image of install.sh:
#   curl -fsSL https://raw.githubusercontent.com/Regent33/Regent/main/scripts/uninstall.sh | sh
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
# `pkill -x` matched on NAME alone, so uninstalling one Regent stopped another
# install's daemons. Stopping processes exists to unlock the files about to be
# deleted, and only a process running FROM this tree can lock them. Where the
# image path is readable (/proc on Linux, `ps -o comm=` on macOS) it decides;
# where it is not, we stop it as before rather than leave a lock behind.
regent_exe_path() {
  _exe=""
  [ -r "/proc/$1/exe" ] && _exe="$(readlink -f "/proc/$1/exe" 2>/dev/null || true)"
  [ -n "$_exe" ] || _exe="$(ps -p "$1" -o comm= 2>/dev/null || true)"
  case "$_exe" in
    */*) printf '%s' "$_exe" ;;
    *) printf '' ;;  # a bare name (Linux `comm`) tells us nothing
  esac
}

# Exact-name matching that holds on procps, macOS AND busybox.
#
# `pgrep -x` is not portable. On a busybox userland (Alpine, most minimal
# container images) `pgrep -x regent-deacon` matches NOTHING for a process
# whose /proc/<pid>/comm is exactly `regent-deacon`, while a loose `pgrep`
# matches it — verified directly. So `-x` alone quietly turned this whole loop
# into a no-op there, and nothing was ever stopped before the files were
# deleted. Loose matching alone is too eager, so fall back to it and require an
# exact `comm`; where /proc is absent (macOS) `-x` already works.
regent_pids() {
  _pids="$(pgrep -x "$1" 2>/dev/null || true)"
  if [ -n "$_pids" ]; then
    printf '%s' "$_pids"
    return
  fi
  for _p in $(pgrep "$1" 2>/dev/null || true); do
    [ "$(cat "/proc/$_p/comm" 2>/dev/null || true)" = "$1" ] && printf '%s ' "$_p"
  done
}
for name in regent-deacon regent-gateway regent-voice-server regent-cli; do
  for pid in $(regent_pids "$name"); do
    exe="$(regent_exe_path "$pid")"
    case "$exe" in
      "$BIN_DIR"/* | "$HOME_DIR"/* | "")
        kill "$pid" 2>/dev/null && echo "→ stopped $name (pid $pid)" || true ;;
      *)
        echo "→ left $name (pid $pid) running — it belongs to another Regent install" ;;
    esac
  done
done

# 2) Remove binaries + shim link.
if [ -d "$BIN_DIR" ]; then
  rm -rf "$BIN_DIR"
  echo "✓ removed $BIN_DIR"
fi
# Only remove the link if it is ours (points into BIN_DIR) or is dangling. The
# comment already said so; the code removed ANY symlink of that name, so
# uninstalling one install took away the `regent` command belonging to another.
if [ -L "$LINK_DIR/regent" ]; then
  target="$(readlink "$LINK_DIR/regent" 2>/dev/null || true)"
  case "$target" in
    "$BIN_DIR"/*)
      rm -f "$LINK_DIR/regent"
      echo "✓ removed $LINK_DIR/regent" ;;
    *)
      if [ ! -e "$LINK_DIR/regent" ]; then
        rm -f "$LINK_DIR/regent"
        echo "✓ removed the dangling $LINK_DIR/regent"
      else
        echo "kept $LINK_DIR/regent — it points at $target, not this install"
      fi ;;
  esac
fi

# 3) Data: keep by default, delete on --purge (includes ~/.regent/src).
#    Onboarding may have pointed the data dir elsewhere (~/.regent/.home) —
#    follow the pointer so purge removes the real home too.
DATA_DIR="$HOME_DIR"
if [ -f "$HOME_DIR/.home" ]; then
  redirected="$(tr -d '\r\n' < "$HOME_DIR/.home" 2>/dev/null || true)"
  [ -n "$redirected" ] && DATA_DIR="$redirected"
fi
if [ "$PURGE" = "1" ]; then
  rm -rf "$DATA_DIR" "$HOME_DIR"
  echo "✓ purged $DATA_DIR (config, keys, sessions, memory, source checkout)"
else
  if [ -d "$DATA_DIR" ]; then
    echo "kept your data at $DATA_DIR (config, keys, sessions, memory)."
    echo "  to delete it too: uninstall.sh --purge   (or: rm -rf $DATA_DIR)"
  fi
fi

echo "✓ Regent uninstalled"

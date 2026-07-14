#!/usr/bin/env sh
# Regent Desktop installer (macOS / Linux) — builds the app AND everything it needs.
#   curl -fsSL https://raw.githubusercontent.com/<owner>/<repo>/main/scripts/install-desktop.sh | sh
# The desktop app is experimental and source-built: it needs the regent-deacon
# agent core (built here into ~/.regent/bin) plus a native bundle produced by
# Tauri (.dmg/.app on macOS, .deb/.AppImage on Linux). Run from a repo checkout,
# or it clones one to ~/.regent/src. Toolchains are checked, never auto-installed.
set -eu

REPO="${REGENT_REPO:-Regent33/Regent}"
BIN_DIR="${REGENT_BIN_DIR:-$HOME/.regent/bin}"
SRC_DIR="${REGENT_SRC_DIR:-$HOME/.regent/src}"

need() { command -v "$1" >/dev/null 2>&1 || { echo "missing prerequisite: $1  -> $2"; exit 1; }; }
echo "→ checking prerequisites…"
need git   "https://git-scm.com"
need cargo "https://rustup.rs"
need bun   "https://bun.sh"
case "$(uname -s)" in
  Linux) echo "  note (Linux): Tauri also needs webkit2gtk-4.1, libgtk-3, librsvg2,"
         echo "        libayatana-appindicator3, and build-essential — install via your package manager first." ;;
esac

# Locate the source: this checkout if we're in one, else clone/update ~/.regent/src.
is_root() { [ -f "$1/src/regent-app/Desktop/src-tauri/tauri.conf.json" ]; }
ROOT=""
probe="$(pwd)"
while [ -n "$probe" ]; do
  if is_root "$probe"; then ROOT="$probe"; break; fi
  parent="$(dirname "$probe")"
  [ "$parent" = "$probe" ] && break
  probe="$parent"
done
if [ -z "$ROOT" ]; then
  echo "→ no local checkout found; cloning $REPO → $SRC_DIR"
  if [ -d "$SRC_DIR/.git" ]; then git -C "$SRC_DIR" pull --ff-only
  else git clone --depth 1 "https://github.com/$REPO" "$SRC_DIR"; fi
  ROOT="$SRC_DIR"
fi
echo "  source: $ROOT"

# 1) Agent core — the app spawns this; without it the app opens but can't chat.
echo "→ building regent-deacon (release)…"
( cd "$ROOT" && cargo build --release -p regent-deacon )
[ -f "$ROOT/target/release/regent-deacon" ] || { echo "deacon build produced no binary"; exit 1; }
mkdir -p "$BIN_DIR"
cp "$ROOT/target/release/regent-deacon" "$BIN_DIR/"
chmod +x "$BIN_DIR/regent-deacon"
echo "  installed deacon → $BIN_DIR"

# The installed app lives outside the repo, so target/ discovery won't reach the
# deacon. GUI apps often don't inherit shell PATH either, so pin the path via a
# shell profile line (best-effort) AND the current session. macOS GUI-launched
# apps may still miss it — launch from a terminal, or set it in your login env.
DEACON="$BIN_DIR/regent-deacon"
export REGENT_DEACON_PATH="$DEACON"
PROFILE="${HOME}/.zprofile"; [ -n "${BASH_VERSION:-}" ] && PROFILE="${HOME}/.bash_profile"
LINE="export REGENT_DEACON_PATH=\"$DEACON\""
if [ -f "$PROFILE" ] && grep -qF "REGENT_DEACON_PATH" "$PROFILE"; then :; else echo "$LINE" >> "$PROFILE"; fi
echo "  REGENT_DEACON_PATH → $DEACON  (added to $PROFILE)"

# 2) Desktop bundle — bun install, then Tauri build (frontend + native bundle).
DESKTOP="$ROOT/src/regent-app/Desktop"
echo "→ building the desktop app (this takes a few minutes)…"
( cd "$DESKTOP" && bun install && bun run tauri build )

# 3) Point the user at what Tauri produced.
BUNDLE="$DESKTOP/src-tauri/target/release/bundle"
echo ""
echo "Regent Desktop built. Artifacts under:"
echo "  $BUNDLE"
found=""
for pat in "$BUNDLE"/dmg/*.dmg "$BUNDLE"/macos/*.app "$BUNDLE"/deb/*.deb "$BUNDLE"/appimage/*.AppImage; do
  for f in $pat; do [ -e "$f" ] && { echo "  • $f"; found=1; }; done
done
[ -z "$found" ] && echo "  (no bundle matched — see $BUNDLE, or run the binary in target/release/ directly)"
echo "  deacon: $DEACON (REGENT_DEACON_PATH set)"
case "$(uname -s)" in
  Darwin) echo "Install: open the .dmg and drag Regent to Applications (or run the .app)." ;;
  Linux)  echo "Install: sudo dpkg -i the .deb, or chmod +x and run the .AppImage." ;;
esac

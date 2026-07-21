#!/usr/bin/env sh
# Regent one-line installer (macOS / Linux):
#   curl -fsSL https://raw.githubusercontent.com/Regent33/Regent/main/scripts/install.sh | sh
# Downloads the latest GitHub release for your OS/arch into ~/.regent/bin and
# links `regent` onto your PATH. Override the repo with REGENT_REPO=owner/repo.
set -eu

REPO="${REGENT_REPO:-Regent33/Regent}"
BIN_DIR="${REGENT_BIN_DIR:-$HOME/.regent/bin}"
LINK_DIR="${REGENT_LINK_DIR:-$HOME/.local/bin}"

case "$(uname -s)" in
  Darwin) os="macos" ;;
  Linux)  os="linux" ;;
  *) echo "unsupported OS: $(uname -s) — build from source (see README)"; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64|amd64)  arch="x86_64" ;;
  arm64|aarch64) arch="aarch64" ;;
  *) echo "unsupported arch: $(uname -m) — build from source (see README)"; exit 1 ;;
esac

mkdir -p "$BIN_DIR" "$LINK_DIR"

# Offline path: the GUI installer bundles the release archive and points us at
# it via REGENT_LOCAL_ARCHIVE, so no network or download is needed.
if [ -n "${REGENT_LOCAL_ARCHIVE:-}" ] && [ -f "$REGENT_LOCAL_ARCHIVE" ]; then
  echo "→ installing from local archive (offline): $REGENT_LOCAL_ARCHIVE"
  tar -xzf "$REGENT_LOCAL_ARCHIVE" -C "$BIN_DIR"
  chmod +x "$BIN_DIR/regent-cli" "$BIN_DIR/regent-deacon" 2>/dev/null || true
else
  asset="regent-${os}-${arch}.tar.gz"
  url="https://github.com/${REPO}/releases/latest/download/${asset}"
  echo "→ downloading ${asset} from ${REPO} (latest release)…"
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  if curl -fSL --progress-bar "$url" -o "$tmp/$asset"; then
    tar -xzf "$tmp/$asset" -C "$BIN_DIR"
    chmod +x "$BIN_DIR/regent-cli" "$BIN_DIR/regent-deacon" 2>/dev/null || true
  else
    # No release asset (yet) → fall back to building from source, Hermes-style.
    echo "no prebuilt release for ${os}-${arch} — building from source instead"
    command -v git   >/dev/null || { echo "need git:  https://git-scm.com"; exit 1; }
    command -v cargo >/dev/null || { echo "need Rust: https://rustup.rs"; exit 1; }
    command -v bun   >/dev/null || { echo "need Bun:  https://bun.sh"; exit 1; }
    src="${REGENT_SRC_DIR:-$HOME/.regent/src}"
    if [ -d "$src/.git" ]; then git -C "$src" pull --ff-only
    else git clone --depth 1 "https://github.com/${REPO}" "$src"; fi
    (cd "$src" && cargo build --release -p regent-deacon)
    (cd "$src/src/regent-cli" && bun install && bun run compile)
    cp "$src/target/release/regent-deacon" "$src/src/regent-cli/dist/regent-cli" "$BIN_DIR/"
  fi
fi
echo "✓ installed to $BIN_DIR"

# Optional: ffmpeg for local webcam capture (camera_capture outside a live
# call). Dropped as a per-user STATIC binary into $BIN_DIR next to the other
# binaries — no sudo, no package manager, and resolve_ffmpeg() finds it beside
# the deacon regardless of install location. Best-effort and non-fatal: any
# failure just falls back to a hint (Regent runs fine without it). Skipped if
# already present, a system ffmpeg is on PATH, or REGENT_NO_FFMPEG is set.
install_ffmpeg() {
  [ -n "${REGENT_NO_FFMPEG:-}" ] && return 0
  target="$BIN_DIR/ffmpeg"
  [ -x "$target" ] && return 0
  command -v ffmpeg >/dev/null 2>&1 && return 0

  url=""; kind=""
  case "$(uname -s)/$(uname -m)" in
    Linux/x86_64|Linux/amd64) url="https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz"; kind="txz" ;;
    Linux/aarch64|Linux/arm64) url="https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-arm64-static.tar.xz"; kind="txz" ;;
    Darwin/*) url="https://evermeet.cx/ffmpeg/getrelease/ffmpeg/zip"; kind="zip" ;;
  esac
  [ -z "$url" ] && { ffmpeg_hint; return 0; }

  if command -v curl >/dev/null 2>&1; then dl() { curl -fsSL -o "$1" "$2"; }
  elif command -v wget >/dev/null 2>&1; then dl() { wget -qO "$1" "$2"; }
  else ffmpeg_hint; return 0; fi

  echo "-> fetching ffmpeg for camera capture (optional)..."
  tmp="$(mktemp -d 2>/dev/null)" || { ffmpeg_hint; return 0; }
  if [ "$kind" = txz ]; then
    dl "$tmp/ff.tar.xz" "$url" && tar -xf "$tmp/ff.tar.xz" -C "$tmp" 2>/dev/null
  else
    dl "$tmp/ff.zip" "$url" && command -v unzip >/dev/null 2>&1 && unzip -qo "$tmp/ff.zip" -d "$tmp" 2>/dev/null
  fi
  f="$(find "$tmp" -type f -name ffmpeg 2>/dev/null | head -n1)"
  if [ -n "$f" ] && install -m 0755 "$f" "$target" 2>/dev/null; then
    echo "   camera ready (ffmpeg -> $target)"
  else
    ffmpeg_hint
  fi
  rm -rf "$tmp"
}

ffmpeg_hint() {
  case "$(uname -s)" in
    Darwin) echo "note: for camera capture, install ffmpeg: brew install ffmpeg" ;;
    *)      echo "note: for camera capture, install ffmpeg (e.g. sudo apt install ffmpeg)" ;;
  esac
}

install_ffmpeg

# The link into LINK_DIR is what puts `regent` on PATH, so REGENT_NO_PATH (set
# by the GUI installer when "add to PATH" is unticked) skips it. The CLI finds
# regent-deacon as a sibling binary, so both live in BIN_DIR either way.
if [ -z "${REGENT_NO_PATH:-}" ]; then
  ln -sf "$BIN_DIR/regent-cli" "$LINK_DIR/regent"
  echo "✓ linked: $LINK_DIR/regent"
  case ":$PATH:" in
    *":$LINK_DIR:"*) ;;
    *) echo "note: $LINK_DIR is not on PATH — add this to your shell profile:"
       echo "  export PATH=\"$LINK_DIR:\$PATH\"" ;;
  esac
fi
echo "Next: just run \`regent\` — setup walks you through it on first launch."

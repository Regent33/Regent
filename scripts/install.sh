#!/usr/bin/env sh
# Regent one-line installer (macOS / Linux):
#   curl -fsSL https://raw.githubusercontent.com/<owner>/<repo>/main/scripts/install.sh | sh
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

asset="regent-${os}-${arch}.tar.gz"
url="https://github.com/${REPO}/releases/latest/download/${asset}"

echo "→ downloading ${asset} from ${REPO} (latest release)…"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
if curl -fSL --progress-bar "$url" -o "$tmp/$asset"; then
  mkdir -p "$BIN_DIR" "$LINK_DIR"
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
  mkdir -p "$BIN_DIR" "$LINK_DIR"
  cp "$src/target/release/regent-deacon" "$src/src/regent-cli/dist/regent-cli" "$BIN_DIR/"
fi
# The CLI finds regent-deacon as a sibling binary, so both live in BIN_DIR.
ln -sf "$BIN_DIR/regent-cli" "$LINK_DIR/regent"

echo "✓ installed to $BIN_DIR"
echo "✓ linked: $LINK_DIR/regent"
case ":$PATH:" in
  *":$LINK_DIR:"*) ;;
  *) echo "note: $LINK_DIR is not on PATH — add this to your shell profile:"
     echo "  export PATH=\"$LINK_DIR:\$PATH\"" ;;
esac
echo "Next: just run \`regent\` — setup walks you through it on first launch."

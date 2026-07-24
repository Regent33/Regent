#!/usr/bin/env sh
# Regent one-line installer (macOS / Linux):
#   curl -fsSL https://raw.githubusercontent.com/Regent33/Regent/main/scripts/install.sh | sh
# Downloads the latest GitHub release for your OS/arch into ~/.regent/bin and
# links `regent` onto your PATH. Override the repo with REGENT_REPO=owner/repo.
set -eu

REPO="${REGENT_REPO:-Regent33/Regent}"
HOME_DIR="${REGENT_HOME:-$HOME/.regent}"
BIN_DIR="${REGENT_BIN_DIR:-$HOME_DIR/bin}"
LINK_DIR="${REGENT_LINK_DIR:-$HOME/.local/bin}"

# SHA-256 of a file as bare lowercase hex, using whatever is on the box. Prints
# nothing if none of the three tools exist — the caller treats that as "cannot
# verify" and fails closed rather than installing something it can't check.
sha256_hex() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  elif command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha256 "$1" | awk '{print $NF}'
  fi
}

# Verify $1 against the expected hex in $2. Fail closed: no tool, empty
# expectation, or mismatch all return non-zero so the caller aborts.
verify_sha256() {
  actual="$(sha256_hex "$1" | tr 'A-F' 'a-f')"
  if [ -z "$actual" ]; then
    echo "✗ cannot verify download: no sha256sum/shasum/openssl found" >&2
    return 1
  fi
  expected="$(printf '%s' "$2" | tr 'A-F' 'a-f')"
  [ -n "$expected" ] && [ "$actual" = "$expected" ]
}

# Secure rollout bridge: v0.1.1 predates sidecars, so pin its GitHub-provided
# asset digests. Future releases use their workflow-generated `.sha256` files.
legacy_sha256() {
  [ "$REPO" = "Regent33/Regent" ] || return 1
  case "$1" in
    regent-linux-x86_64.tar.gz) echo "2a71791dff528643f9f6dcaa9b438bd9a52d99f5a58e6fff223f4ebd30c8b5d8" ;;
    regent-macos-aarch64.tar.gz) echo "07b416fb368c7848f2d017b5ccc8f3a4cd2d0f19c5d5e0f8fd19f5458584fff5" ;;
    *) return 1 ;;
  esac
}

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
# it via REGENT_LOCAL_ARCHIVE, so no network or download is needed. Its payload
# is trusted through the attested installer build, so this path deliberately
# skips the network checksum below.
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
  from_source=0
  if curl -fSL --progress-bar "$url" -o "$tmp/$asset"; then
    if curl -fsSL "$url.sha256" -o "$tmp/$asset.sha256"; then
      expected="$(awk '{print $1; exit}' "$tmp/$asset.sha256")"
    else
      expected="$(legacy_sha256 "$asset" 2>/dev/null || true)"
      [ -n "$expected" ] && echo "✓ using the pinned v0.1.1 checksum for ${asset}"
    fi
    case "$expected" in *[!0-9a-fA-F]*|'') from_source=1 ;; esac
    [ "${#expected}" -eq 64 ] || from_source=1
    if [ "$from_source" -eq 0 ] && verify_sha256 "$tmp/$asset" "$expected"; then
      echo "✓ verified ${asset} (sha256)"
      tar -xzf "$tmp/$asset" -C "$BIN_DIR"
      chmod +x "$BIN_DIR/regent-cli" "$BIN_DIR/regent-deacon" 2>/dev/null || true
    else
      echo "✗ no valid checksum for ${asset} — refusing the archive" >&2
      from_source=1
    fi
  else
    from_source=1
  fi
  if [ "$from_source" -eq 1 ]; then
    echo "no verified release for ${os}-${arch} — building from source instead"
    command -v git   >/dev/null || { echo "need git:  https://git-scm.com"; exit 1; }
    command -v cargo >/dev/null || { echo "need Rust: https://rustup.rs"; exit 1; }
    command -v bun   >/dev/null || { echo "need Bun:  https://bun.sh"; exit 1; }
    src="${REGENT_SRC_DIR:-$HOME_DIR/src}"
    if [ -d "$src/.git" ]; then git -C "$src" pull --ff-only
    else git clone --depth 1 "https://github.com/${REPO}" "$src"; fi
    (cd "$src" && cargo build --release -p regent-deacon)
    (cd "$src/src/regent-cli" && bun install && bun run compile)
    cp "$src/target/release/regent-deacon" "$src/src/regent-cli/dist/regent-cli" "$BIN_DIR/"
  fi
fi
echo "✓ installed to $BIN_DIR"

# Optional: ffmpeg for local webcam capture (camera_capture outside a live
# call). Best-effort and NON-FATAL — Regent runs fine without it, and the camera
# tool prints a hint when it's absent. We do NOT auto-download it on macOS/Linux:
# the static builds those used to come from (johnvansickle / evermeet) publish
# only a rolling "latest" at a mutable URL with a same-URL checksum, which is
# exactly the artifact we cannot pin to a reviewed hash — fetching it to run
# unverified is the supply-chain hole this closes. The system package manager is
# the trustworthy source, so point at it instead. Skipped entirely if ffmpeg is
# already present or REGENT_NO_FFMPEG is set.
install_ffmpeg() {
  [ -n "${REGENT_NO_FFMPEG:-}" ] && return 0
  [ -x "$BIN_DIR/ffmpeg" ] && return 0
  command -v ffmpeg >/dev/null 2>&1 && return 0
  ffmpeg_hint
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
echo "If setup does not open here, run \`regent\` to start it."

# When a person is driving an interactive install, run first-time setup for them
# right now instead of leaving it for the next launch. Skipped for the GUI /
# offline embedding (REGENT_LOCAL_ARCHIVE) and when the caller opts out
# (REGENT_NO_LAUNCH). `curl | sh` feeds the script itself in on stdin, so a bare
# read would consume script bytes — reattach the real terminal from /dev/tty. No
# controlling TTY (CI, cron, a pipe with nothing behind it) → skip cleanly.
run_setup() {
  if [ -n "${REGENT_LOCAL_ARCHIVE:-}" ] || [ -n "${REGENT_NO_LAUNCH:-}" ]; then
    return 0
  fi
  cli="$BIN_DIR/regent-cli"
  [ -x "$cli" ] || return 0
  if [ -t 0 ]; then
    "$cli" setup
  elif (exec </dev/tty) 2>/dev/null; then
    echo "→ starting first-time setup…"
    "$cli" setup </dev/tty
  fi
}
run_setup

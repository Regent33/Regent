#!/usr/bin/env sh
# Hermetic checks for the real POSIX installer. Fake curl/build tools keep the
# production download branch local; offline cases use a scratch archive/home.
set -u
root="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
pass=0
fail=0
ok() {
  if [ "$2" -eq 0 ]; then
    pass=$((pass + 1)); echo "  PASS $1"
  else
    fail=$((fail + 1)); echo "  FAIL $1"
  fi
}

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
payload="$work/payload"
mkdir -p "$payload"
printf '#!/bin/sh\necho cli\n' >"$payload/regent-cli"
printf '#!/bin/sh\necho deacon\n' >"$payload/regent-deacon"
archive="$work/regent-local.tar.gz"
tar -czf "$archive" -C "$payload" .
hash="$(sha256sum "$archive" | cut -d ' ' -f 1)"
printf '%s  fixture.tar.gz\n' "$hash" >"$work/good.sha256"
printf '%064d  fixture.tar.gz\n' 0 >"$work/bad.sha256"

# Replace only the external download/build commands. install.sh itself still
# chooses assets, validates the sidecar, extracts, and decides whether to fall
# back. The blocked build tools make a refusal terminate without any network.
fakebin="$work/fake-bin"
mkdir -p "$fakebin"
cat >"$fakebin/curl" <<'EOF'
#!/bin/sh
out= url=
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) out="$2"; shift 2 ;;
    -*) shift ;;
    *) url="$1"; shift ;;
  esac
done
[ -n "$out" ] && [ -n "$url" ] || exit 2
case "$url" in
  *.sha256)
    [ "$REGENT_TEST_CHECKSUM_MODE" = missing ] && exit 22
    cp "$REGENT_TEST_CHECKSUM_FILE" "$out"
    ;;
  *) cp "$REGENT_TEST_ARCHIVE" "$out" ;;
esac
EOF
cat >"$fakebin/blocked" <<'EOF'
#!/bin/sh
echo 'source fallback blocked by installer test' >&2
exit 97
EOF
cat >"$fakebin/uname" <<'EOF'
#!/bin/sh
case "${1:-}" in -m) echo x86_64 ;; *) echo Linux ;; esac
EOF
chmod +x "$fakebin/curl" "$fakebin/blocked" "$fakebin/uname"
for tool in git cargo bun; do cp "$fakebin/blocked" "$fakebin/$tool"; done

network_install() {
  mode="$1"; bin="$2"; checksum="$3"
  home="$work/home-$mode"
  PATH="$fakebin:$PATH" HOME="$home" REGENT_REPO="tests/Regent" \
    REGENT_BIN_DIR="$bin" REGENT_LINK_DIR="$home/.local/bin" \
    REGENT_SRC_DIR="$home/src" REGENT_NO_PATH=1 REGENT_NO_FFMPEG=1 \
    REGENT_NO_LAUNCH=1 REGENT_TEST_ARCHIVE="$archive" \
    REGENT_TEST_CHECKSUM_MODE="$mode" REGENT_TEST_CHECKSUM_FILE="$checksum" \
    sh "$root/scripts/install.sh" >"$work/$mode.log" 2>&1
}
offline_install() {
  bin="$1"; home="$work/offline-home"
  PATH="$fakebin:$PATH" HOME="$home" REGENT_BIN_DIR="$bin" \
    REGENT_LINK_DIR="$home/.local/bin" REGENT_LOCAL_ARCHIVE="$archive" \
    REGENT_NO_PATH=1 REGENT_NO_FFMPEG=1 REGENT_NO_LAUNCH=1 \
    sh "$root/scripts/install.sh" >"$work/offline.log" 2>&1
}
offline_home_install() {
  home="$1"
  PATH="$fakebin:$PATH" HOME="$work/process-home" REGENT_HOME="$home" \
    REGENT_LINK_DIR="$home/.local/bin" REGENT_LOCAL_ARCHIVE="$archive" \
    REGENT_NO_PATH=1 REGENT_NO_FFMPEG=1 REGENT_NO_LAUNCH=1 \
    sh "$root/scripts/install.sh" >"$work/offline-home.log" 2>&1
}
installed() { [ -f "$1/regent-cli" ] && [ -f "$1/regent-deacon" ]; }

printf '%s\n' 'install.sh - verified download branch'
good_bin="$work/good/bin"
network_install good "$good_bin" "$work/good.sha256"; code=$?
ok 'valid checksum exits 0' "$code"
installed "$good_bin"; ok 'valid archive is extracted' $?
grep -q 'verified .*sha256' "$work/good.log"; ok 'verification is reported' $?

for mode in mismatch missing; do
  bin="$work/$mode/bin"
  checksum="$work/bad.sha256"
  network_install "$mode" "$bin" "$checksum"; code=$?
  [ "$code" -ne 0 ]; ok "$mode checksum refuses the install" $?
  [ ! -f "$bin/regent-cli" ]; ok "$mode checksum never extracts the archive" $?
  grep -q 'refusing the archive' "$work/$mode.log"; ok "$mode refusal is explicit" $?
done

printf '%s\n' 'install.sh - offline GUI payload / reinstall'
offline_bin="$work/offline/bin"
offline_install "$offline_bin"; code=$?
ok 'offline payload exits 0' "$code"
installed "$offline_bin"; ok 'offline payload is extracted' $?
grep -q offline "$work/offline.log"; ok 'offline branch is reported' $?
offline_install "$offline_bin"; code=$?
ok 'reinstall over the same directory exits 0' "$code"
installed "$offline_bin"; ok 'reinstall leaves binaries intact' $?
custom_home="$work/custom-home"
offline_home_install "$custom_home"; code=$?
ok 'REGENT_HOME install exits 0' "$code"
installed "$custom_home/bin"; ok 'REGENT_HOME owns the default bin directory' $?

printf '%s\n' 'install.sh - quoted Unicode path'
unicode_bin="$work/Régent 漢字 dir o'brien/bin"
offline_install "$unicode_bin"; code=$?
ok 'Unicode/spaced/quoted path exits 0' "$code"
installed "$unicode_bin"; ok 'Unicode/spaced/quoted path receives binaries' $?

printf '%s\n' 'uninstall.sh - only removes its own link'
# The script's comment already promised "only remove the link if it is ours
# (points into BIN_DIR) or is dangling", but the code removed ANY symlink of
# that name — so uninstalling one install took away the `regent` command
# belonging to another.
un_home="$work/un/home"
un_bin="$un_home/bin"
un_link="$work/un/link"
other_bin="$work/un/other/bin"
mkdir -p "$un_bin" "$un_link" "$other_bin"
printf 'x\n' >"$un_bin/regent-cli"
printf 'x\n' >"$other_bin/regent-cli"
run_uninstall() {
  REGENT_HOME="$un_home" REGENT_BIN_DIR="$un_bin" REGENT_LINK_DIR="$un_link" \
    sh "$root/scripts/uninstall.sh" >"$work/uninstall.log" 2>&1
}
if ln -s "$other_bin/regent-cli" "$un_link/regent" 2>/dev/null && [ -L "$un_link/regent" ]; then
  run_uninstall
  [ -L "$un_link/regent" ]; ok "another install's regent link is kept" $?
  grep -q 'not this install' "$work/uninstall.log"; ok 'keeping the link is reported' $?
  rm -f "$un_link/regent"

  mkdir -p "$un_bin"
  printf 'x\n' >"$un_bin/regent-cli"
  ln -s "$un_bin/regent-cli" "$un_link/regent"
  run_uninstall
  [ ! -e "$un_link/regent" ] && [ ! -L "$un_link/regent" ]; ok 'our own regent link is removed' $?

  ln -s "$work/un/never-existed" "$un_link/regent"
  run_uninstall
  [ ! -L "$un_link/regent" ]; ok 'a dangling regent link is removed' $?
else
  printf '  SKIP symlinks unavailable on this filesystem\n'
fi

# Data is kept unless --purge, on every one of those runs.
[ -d "$un_home" ]; ok 'uninstall keeps your data by default' $?

printf '\nverify-install.sh: %s passed, %s failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]

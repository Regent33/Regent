# Regent Desktop installer (Windows) — builds the app AND everything it needs.
#   irm https://raw.githubusercontent.com/<owner>/<repo>/main/scripts/install-desktop.ps1 | iex
# The desktop app is experimental and source-built: it needs the regent-deacon
# agent core (built here into ~/.regent/bin) plus a native Windows installer
# produced by Tauri. Run from a repo checkout, or it clones one to ~/.regent/src.
# Toolchains are checked (with install URLs), never silently installed.
$ErrorActionPreference = "Stop"

$repo   = if ($env:REGENT_REPO)     { $env:REGENT_REPO }     else { "Regent33/Regent" }
$binDir = if ($env:REGENT_BIN_DIR)  { $env:REGENT_BIN_DIR }  else { Join-Path $env:USERPROFILE ".regent\bin" }
$srcDir = if ($env:REGENT_SRC_DIR)  { $env:REGENT_SRC_DIR }  else { Join-Path $env:USERPROFILE ".regent\src" }

function Need($cmd, $url) {
  if (-not (Get-Command $cmd -ErrorAction SilentlyContinue)) {
    Write-Host "missing prerequisite: $cmd  -> $url" -ForegroundColor Red
    exit 1
  }
}
# The tauri build itself needs cargo (Rust shell) + bun (frontend); the deacon
# is provisioned by the main installer below.
Write-Host "-> checking prerequisites..." -ForegroundColor Cyan
Need git   "https://git-scm.com"
Need cargo "https://rustup.rs"
Need bun   "https://bun.sh"

# Locate the source: this checkout if we're in one, else clone/update ~/.regent/src.
function Test-RepoRoot($d) { Test-Path (Join-Path $d "src\regent-app\Desktop\src-tauri\tauri.conf.json") }
$here = (Get-Location).Path
$root = $null
$probe = $here
while ($probe) {
  if (Test-RepoRoot $probe) { $root = $probe; break }
  $parent = Split-Path $probe -Parent
  if ($parent -eq $probe) { break }
  $probe = $parent
}
if (-not $root) {
  Write-Host "-> no local checkout found; cloning $repo -> $srcDir" -ForegroundColor Cyan
  if (Test-Path (Join-Path $srcDir ".git")) { git -C $srcDir pull --ff-only }
  else { git clone --depth 1 "https://github.com/$repo" $srcDir }
  $root = $srcDir
}
Write-Host "   source: $root"

# 1) Agent core — the app spawns this; without it the app opens but can't chat.
#    Delegate to the canonical installer (release-first, source fallback) rather
#    than duplicating its build logic here; skip if already installed.
$deaconPath = Join-Path $binDir "regent-deacon.exe"
if (Test-Path $deaconPath) {
  Write-Host "-> deacon already installed: $deaconPath" -ForegroundColor Cyan
} else {
  Write-Host "-> provisioning the agent core via the main installer..." -ForegroundColor Cyan
  & (Join-Path $root "scripts\install.ps1")
}
if (-not (Test-Path $deaconPath)) { Write-Host "deacon not found after install: $deaconPath" -ForegroundColor Red; exit 1 }

# The installed app lives outside the repo, so target/ discovery won't reach the
# deacon and GUI PATH may not include binDir — pin it explicitly (User env is
# inherited by GUI processes on Windows). This is the reliable seam.
[Environment]::SetEnvironmentVariable("REGENT_DEACON_PATH", $deaconPath, "User")
$env:REGENT_DEACON_PATH = $deaconPath
Write-Host "   set REGENT_DEACON_PATH (user) -> $deaconPath"

# 2) Desktop bundle — bun install, then Tauri build (frontend + native installer).
$desktop = Join-Path $root "src\regent-app\Desktop"
Write-Host "-> building the desktop app (this takes a few minutes)..." -ForegroundColor Cyan
Push-Location $desktop
try {
  bun install
  bun run tauri build
} finally { Pop-Location }

# 3) Find the native installer Tauri produced and hand it to the user.
$bundle = Join-Path $desktop "src-tauri\target\release\bundle"
$installer = Get-ChildItem -Path $bundle -Recurse -Include *.exe, *.msi -ErrorAction SilentlyContinue |
  Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $installer) {
  Write-Host "build finished but no installer was found under $bundle" -ForegroundColor Yellow
  Write-Host "the built app is under $desktop\src-tauri\target\release\ — run Regent.exe directly."
  exit 0
}
Write-Host ""
Write-Host "Regent Desktop built:" -ForegroundColor Green
Write-Host "  $($installer.FullName)"
Write-Host "  deacon: $deaconPath (REGENT_DEACON_PATH set)"
if ($args -contains "--run" -or $args -contains "-Run") {
  Write-Host "-> launching the installer..."
  Start-Process $installer.FullName          # normal Windows install dialog — you confirm
} else {
  Write-Host "Run it to install:  Start-Process `"$($installer.FullName)`"   (or add --run)"
}

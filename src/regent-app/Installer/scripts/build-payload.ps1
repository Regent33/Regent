# Stages everything "Regent Setup" ships inside itself (Windows).
#   pwsh src/regent-app/Installer/scripts/build-payload.ps1
# Produces src-tauri/payload/, which tauri.conf.json bundles as resources:
#   regent-windows-<arch>.zip  deacon + CLI, same shape as the GitHub release
#                              asset, so install.ps1's offline path just works
#   install.ps1                the one-line installer, run with REGENT_LOCAL_ARCHIVE
#   app/Regent.exe             the desktop app, copied to <install_dir>\app
# Skip the slow parts with -SkipCore / -SkipApp when iterating on one of them.
[CmdletBinding()]
param([switch]$SkipCore, [switch]$SkipApp)
$ErrorActionPreference = "Stop"

$installer = Split-Path -Parent $PSScriptRoot
$repo = (Resolve-Path (Join-Path $installer "..\..\..")).Path
$payload = Join-Path $installer "src-tauri\payload"
$arch = if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq "Arm64") { "aarch64" } else { "x86_64" }

New-Item -ItemType Directory -Force $payload, (Join-Path $payload "app") | Out-Null

if (-not $SkipCore) {
  Write-Host "==> deacon + CLI"
  Push-Location $repo
  try {
    cargo build --release -p regent-deacon
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    Push-Location (Join-Path $repo "src\regent-cli")
    try {
      bun install --frozen-lockfile
      if ($LASTEXITCODE -ne 0) { throw "bun install failed" }
      bun run compile
      if ($LASTEXITCODE -ne 0) { throw "bun run compile failed" }
    } finally { Pop-Location }
  } finally { Pop-Location }

  # Archive layout must match the release asset: both binaries at the root, so
  # the CLI still finds regent-deacon as a sibling after extraction.
  $stage = Join-Path $env:TEMP "regent-payload-core"
  Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Force $stage | Out-Null
  Copy-Item (Join-Path $repo "target\release\regent-deacon.exe") $stage
  Copy-Item (Join-Path $repo "src\regent-cli\dist\regent-cli.exe") $stage
  $zip = Join-Path $payload "regent-windows-$arch.zip"
  Remove-Item $zip -Force -ErrorAction SilentlyContinue
  Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip
  Remove-Item -Recurse -Force $stage
}

if (-not $SkipApp) {
  Write-Host "==> desktop app"
  Push-Location (Join-Path $repo "src\regent-app\Desktop")
  try {
    bun install --frozen-lockfile
    if ($LASTEXITCODE -ne 0) { throw "bun install failed" }
    # --no-bundle: we ship the bare exe and do our own placement; an NSIS
    # installer nested inside this installer would be pointless.
    bun run tauri build --no-bundle
    if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }
  } finally { Pop-Location }
  Copy-Item (Join-Path $repo "src\regent-app\Desktop\src-tauri\target\release\regent-desktop.exe") `
    (Join-Path $payload "app\Regent.exe") -Force
}

Copy-Item (Join-Path $repo "scripts\install.ps1") $payload -Force

Write-Host "`npayload ready: $payload"
Get-ChildItem -Recurse -File $payload | ForEach-Object {
  "  {0,-28} {1,8:N1} MB" -f $_.Name, ($_.Length / 1MB)
}

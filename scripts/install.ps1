# Regent one-line installer (Windows PowerShell):
#   irm https://raw.githubusercontent.com/Regent33/Regent/main/scripts/install.ps1 | iex
# Downloads the latest GitHub release into %USERPROFILE%\.regent\bin and puts
# `regent` on your PATH. Override the repo with $env:REGENT_REPO = "owner/repo".
$ErrorActionPreference = "Stop"

$repo = if ($env:REGENT_REPO) { $env:REGENT_REPO } else { "Regent33/Regent" }
$binDir = if ($env:REGENT_BIN_DIR) { $env:REGENT_BIN_DIR } else { Join-Path $env:USERPROFILE ".regent\bin" }

$arch = if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq "Arm64") { "aarch64" } else { "x86_64" }
$asset = "regent-windows-$arch.zip"
$url = "https://github.com/$repo/releases/latest/download/$asset"

Write-Host "-> downloading $asset from $repo (latest release)..."
$tmp = Join-Path $env:TEMP "regent-install.zip"
$fromSource = $false
try {
  Invoke-WebRequest -Uri $url -OutFile $tmp -UseBasicParsing
} catch {
  $fromSource = $true
}

New-Item -ItemType Directory -Force $binDir | Out-Null
if (-not $fromSource) {
  Expand-Archive -Path $tmp -DestinationPath $binDir -Force
  Remove-Item $tmp -Force
} else {
  # No release asset (yet) -> build from source, same as install.sh's fallback.
  Write-Host "no prebuilt release for windows-$arch - building from source instead"
  foreach ($t in @(@('git', 'https://git-scm.com'), @('cargo', 'https://rustup.rs'), @('bun', 'https://bun.sh'))) {
    if (-not (Get-Command $t[0] -ErrorAction SilentlyContinue)) { Write-Host "need $($t[0]): $($t[1])"; exit 1 }
  }
  $src = if ($env:REGENT_SRC_DIR) { $env:REGENT_SRC_DIR } else { Join-Path $env:USERPROFILE ".regent\src" }
  if (Test-Path (Join-Path $src ".git")) { git -C $src pull --ff-only }
  else { git clone --depth 1 "https://github.com/$repo" $src }
  Push-Location $src
  try {
    cargo build --release -p regent-deacon
    Push-Location (Join-Path $src "src\regent-cli")
    try { bun install; bun run compile } finally { Pop-Location }
  } finally { Pop-Location }
  Copy-Item (Join-Path $src "target\release\regent-deacon.exe") $binDir -Force
  Copy-Item (Join-Path $src "src\regent-cli\dist\regent-cli.exe") $binDir -Force
}

# Shim + user PATH (the CLI finds regent-deacon as a sibling binary in binDir).
$shim = Join-Path $binDir "regent.cmd"
"@echo off`r`n`"$binDir\regent-cli.exe`" %*" | Set-Content -Encoding ascii $shim
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if (($userPath -split ";") -notcontains $binDir) {
  [Environment]::SetEnvironmentVariable("Path", "$binDir;$userPath", "User")
  Write-Host "added $binDir to your user PATH (open a new terminal to pick it up)"
}

Write-Host "installed to $binDir"
Write-Host "Next: just run 'regent' - setup walks you through it on first launch."

# Regent one-line installer (Windows PowerShell):
#   irm https://raw.githubusercontent.com/<owner>/<repo>/main/scripts/install.ps1 | iex
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
try {
  Invoke-WebRequest -Uri $url -OutFile $tmp -UseBasicParsing
} catch {
  Write-Host "download failed: $url"
  Write-Host "No release yet, or unsupported platform - build from source instead:"
  Write-Host "  git clone https://github.com/$repo ; cd Regent   # then see README (Install)"
  exit 1
}

New-Item -ItemType Directory -Force $binDir | Out-Null
Expand-Archive -Path $tmp -DestinationPath $binDir -Force
Remove-Item $tmp -Force

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

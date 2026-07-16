# Builds "Regent Setup" - signed automatically when a certificate is
# configured, unsigned (with a warning) when not. This is the one build
# entry point, so signing is never a separate step someone forgets:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-setup.ps1
#
# Configure exactly one of:
#   $env:REGENT_SIGN_THUMBPRINT  SHA-1 thumbprint of an installed Authenticode
#                                cert (classic OV/EV via signtool).
#   $env:REGENT_SIGN_COMMAND     Full sign command with a %1 placeholder for
#                                the file path - the Azure Trusted Signing /
#                                cloud-HSM route, e.g.:
#                                trusted-signing-cli -e https://eus.codesigning.azure.net
#                                  -a <account> -c <profile> %1
#
# NSIS reuses the same command for the uninstaller it writes, and our own
# GUI uninstall.exe is a byte copy of the signed installer - so one signed
# build covers every executable we ship.
#
# ASCII only: Windows PowerShell 5.1 reads a BOM-less .ps1 as ANSI, and any
# smart punctuation in here becomes a parse error.
[CmdletBinding()]
param([switch]$SkipPayload)
$ErrorActionPreference = "Stop"

$installer = Split-Path -Parent $PSScriptRoot
Set-Location $installer

if (-not $SkipPayload) {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "build-payload.ps1")
  if ($LASTEXITCODE -ne 0) { throw "build-payload failed" }
}

$cfg = $null
if ($env:REGENT_SIGN_COMMAND) {
  $cfg = @{ bundle = @{ windows = @{ signCommand = $env:REGENT_SIGN_COMMAND } } }
  Write-Host "==> signing via REGENT_SIGN_COMMAND"
} elseif ($env:REGENT_SIGN_THUMBPRINT) {
  $cfg = @{ bundle = @{ windows = @{ certificateThumbprint = $env:REGENT_SIGN_THUMBPRINT } } }
  Write-Host "==> signing with certificate $($env:REGENT_SIGN_THUMBPRINT)"
} else {
  Write-Warning "no REGENT_SIGN_THUMBPRINT / REGENT_SIGN_COMMAND set - building UNSIGNED (SmartScreen will warn users)"
}

bun install --frozen-lockfile
if ($LASTEXITCODE -ne 0) { throw "bun install failed" }

if ($cfg) {
  # --config merges over tauri.conf.json; ConvertTo-Json keeps the quoting sane
  # across the PowerShell -> bun -> tauri argv boundary.
  bun run tauri build --config ($cfg | ConvertTo-Json -Compress -Depth 5)
} else {
  bun run tauri build
}
if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }

$exe = Get-ChildItem "src-tauri\target\release\bundle\nsis\*.exe" | Select-Object -First 1
$sig = Get-AuthenticodeSignature $exe.FullName
Write-Host ""
Write-Host ("built:  {0}  ({1:N1} MB)" -f $exe.Name, ($exe.Length / 1MB))
Write-Host ("signed: {0}" -f $sig.Status)
if ($sig.Status -eq "Valid") { Write-Host ("        {0}" -f $sig.SignerCertificate.Subject) }

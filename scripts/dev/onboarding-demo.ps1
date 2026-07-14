# See the Regent first-run onboarding exactly as a new user would — in a
# sandboxed REGENT_HOME, so your real ~/.regent is never touched.
#   powershell -ExecutionPolicy Bypass -File scripts\dev\onboarding-demo.ps1
# Walk the wizard, land in chat, Ctrl+C when done. Re-run for a fresh run.
$ErrorActionPreference = "Stop"

$repo = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$exe = Join-Path $repo "src\regent-cli\dist\regent-cli.exe"
if (-not (Test-Path $exe)) {
  Write-Host "dist binary missing - build it first:  cd src\regent-cli ; bun run compile"
  exit 1
}

$demoHome = Join-Path $env:TEMP ("regent-onboarding-demo-" + (Get-Date -Format "HHmmss"))
Write-Host ""
Write-Host "=== Regent onboarding demo (sandboxed) ===" -ForegroundColor Cyan
Write-Host "REGENT_HOME = $demoHome  (your real ~/.regent is untouched)"
Write-Host "Tip: pick 'ollama' to try the local no-key path. Ctrl+C exits chat."
Write-Host ""

$env:REGENT_HOME = $demoHome
& $exe   # bare `regent` -> first-run wizard -> chat

Write-Host ""
Write-Host "Demo home left at $demoHome - inspect config.yaml/.env, delete when done:"
Write-Host "  Remove-Item -Recurse -Force `"$demoHome`""

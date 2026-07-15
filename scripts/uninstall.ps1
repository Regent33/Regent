# Regent uninstaller (Windows PowerShell) — mirror image of install.ps1:
#   irm https://raw.githubusercontent.com/Regent33/Regent/main/scripts/uninstall.ps1 | iex
# Stops Regent processes, removes %USERPROFILE%\.regent\bin (binaries + shim)
# and the user PATH entry. Your data in %USERPROFILE%\.regent (config, keys,
# sessions, memory) is KEPT unless you set $env:REGENT_PURGE = "1" first.
# Idempotent: safe to run twice, or after a partial install.
$ErrorActionPreference = "Continue"

$homeDir = if ($env:REGENT_HOME) { $env:REGENT_HOME } else { Join-Path $env:USERPROFILE ".regent" }
$binDir = if ($env:REGENT_BIN_DIR) { $env:REGENT_BIN_DIR } else { Join-Path $homeDir "bin" }
$purge = ($env:REGENT_PURGE -eq "1") -or ($args -contains "--purge") -or ($args -contains "-Purge")

# 1) Stop running Regent processes (fine if none are running — also while the
#    app/CLI is mid-run, so the binaries below aren't locked).
foreach ($name in "regent-deacon", "regent-gateway", "regent-voice-server", "regent-cli") {
  Get-Process -Name $name -ErrorAction SilentlyContinue | ForEach-Object {
    Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
    Write-Host "-> stopped $name (pid $($_.Id))"
  }
}
Get-ChildItem -Path $homeDir -Filter "*.pid" -ErrorAction SilentlyContinue |
  Remove-Item -Force -ErrorAction SilentlyContinue

# 2) Remove binaries + shim.
if (Test-Path $binDir) {
  Remove-Item -Recurse -Force $binDir
  Write-Host "removed $binDir"
}

# 3) Remove the user PATH entry the installer added.
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath) {
  $newPath = ($userPath -split ";" | Where-Object { $_ -and $_ -ne $binDir }) -join ";"
  if ($newPath -ne $userPath) {
    [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
    Write-Host "removed $binDir from your user PATH"
  }
}

# 4) Data: keep by default, delete on purge (includes .regent\src).
#    Onboarding may have pointed the data dir elsewhere (.regent\.home) —
#    follow the pointer so purge removes the real home too.
$dataDir = $homeDir
$pointer = Join-Path $homeDir ".home"
if (Test-Path $pointer) {
  $redirected = (Get-Content $pointer -Raw -ErrorAction SilentlyContinue)
  if ($redirected) { $redirected = $redirected.Trim() }
  if ($redirected) { $dataDir = $redirected }
}
if ($purge) {
  foreach ($d in @($dataDir, $homeDir) | Select-Object -Unique) {
    if (Test-Path $d) {
      Remove-Item -Recurse -Force $d
      Write-Host "purged $d (config, keys, sessions, memory, source checkout)"
    }
  }
} elseif (Test-Path $dataDir) {
  Write-Host "kept your data at $dataDir (config, keys, sessions, memory)."
  Write-Host "  to delete it too: `$env:REGENT_PURGE = '1'; then re-run this script"
}

Write-Host "Regent uninstalled"

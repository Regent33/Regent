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

# Decide whether the persisted REGENT_DEACON_PATH pin belongs to the install we
# are removing (or has gone stale) and should be cleared, or points at a
# *different*, still-valid Regent that must be preserved. Kept pure and
# filesystem-only so the uninstall test can exercise every branch without ever
# touching the real registry:
#   - a pin inside the bin dir we are deleting     -> remove (this install)
#   - a pin whose target no longer exists on disk  -> remove (stale)
#   - a pin resolving to another install's deacon  -> keep (not ours)
function Test-RegentPinRemovable($pin, $binDir) {
  if (-not $pin) { return $false }
  try {
    $expanded = [Environment]::ExpandEnvironmentVariables($pin.Trim('"'))
    $pinNorm = [IO.Path]::GetFullPath($expanded).TrimEnd('\', '/')
    $binNorm = [IO.Path]::GetFullPath($binDir).TrimEnd('\', '/')
    if ($pinNorm.StartsWith("$binNorm\", [StringComparison]::OrdinalIgnoreCase)) { return $true }
    return (-not (Test-Path -LiteralPath $pinNorm -PathType Leaf -ErrorAction Stop))
  } catch { return $true }
}

# Test seam: source the pure decision without process or registry side effects.
if ($env:REGENT_TEST_UNINSTALL_LIB_ONLY) { return }

# 1) Stop running Regent processes (fine if none are running — also while the
#    app/CLI is mid-run, so the binaries below aren't locked). "Regent" is the
#    desktop app (Regent.exe); it spawns the deacon, so it goes first.
foreach ($name in "Regent", "regent-deacon", "regent-gateway", "regent-voice-server", "regent-cli") {
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

# 3) Remove the user PATH entry the installer added, plus the deacon pin the GUI
#    installer set (REGENT_DEACON_PATH) — harmless if it was never there.
#
#    Read the RAW registry value and write the SAME kind back. The obvious
#    [Environment]::GetEnvironmentVariable('Path','User') EXPANDS every %VAR% and
#    SetEnvironmentVariable stores REG_SZ, which bakes those %VAR% into today's
#    value and permanently downgrades the key from REG_EXPAND_SZ so later ones
#    stop expanding too. Uninstalling Regent is no excuse to damage someone's
#    PATH. Mirrors Add-UserPath in install.ps1.
$key = Get-Item 'HKCU:\Environment'
$raw = $key.GetValue('Path', '', 'DoNotExpandEnvironmentNames')
$pathChanged = $false
if ($raw) {
  $kind = try { $key.GetValueKind('Path') } catch { 'ExpandString' }
  $newPath = ($raw -split ';' | Where-Object { $_ -and $_ -ne $binDir }) -join ';'
  if ($newPath -ne $raw) {
    Set-ItemProperty 'HKCU:\Environment' -Name Path -Value $newPath -Type $kind
    Write-Host "removed $binDir from your user PATH"
    $pathChanged = $true
  }
}
$pin = $key.GetValue('REGENT_DEACON_PATH', $null, 'DoNotExpandEnvironmentNames')
if (Test-RegentPinRemovable $pin $binDir) {
  Remove-ItemProperty 'HKCU:\Environment' -Name 'REGENT_DEACON_PATH' -ErrorAction SilentlyContinue
  Write-Host "removed the REGENT_DEACON_PATH pin (this install, or its target is gone)"
  $pathChanged = $true
} elseif ($pin) {
  Write-Host "kept REGENT_DEACON_PATH - it points at a different Regent install"
}

# A raw registry write does not broadcast WM_SETTINGCHANGE, so without this
# Explorer keeps handing new shells its cached environment until the next
# sign-out. Mirrors install.ps1. (SMTO_ABORTIFHUNG, 5s: a hung window must not
# hang the uninstall.)
if ($pathChanged) {
  if (-not ('Regent.Env' -as [type])) {
    Add-Type -Namespace Regent -Name Env -MemberDefinition @'
[DllImport("user32.dll", SetLastError=true, CharSet=CharSet.Auto)]
public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam,
  string lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);
'@
  }
  $out = [UIntPtr]::Zero
  [void][Regent.Env]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero,
    'Environment', 2, 5000, [ref]$out)
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

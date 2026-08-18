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

# Whether a running process belongs to the install being removed, decided from
# its image path alone.
#
# Matching on NAME alone made uninstalling one Regent stop another one's
# daemons: removing a sandbox install under $env:TEMP stopped the
# regent-voice-server of the real install in %LOCALAPPDATA%. Stopping processes
# exists to unlock the files about to be deleted, and only a process running
# FROM this tree can lock them.
#
# An unreadable path counts as ours: it is almost always our own elevated
# process, it may hold a lock, and Stop-Process on somebody else's is refused
# by the OS anyway. Pure and filesystem-only so the tests can drive it.
function Test-RegentProcessOwned($path, $roots) {
  if (-not $path) { return $true }
  try { $full = [IO.Path]::GetFullPath($path) } catch { return $true }
  foreach ($root in $roots) {
    if (-not $root) { continue }
    try { $norm = [IO.Path]::GetFullPath($root).TrimEnd('\', '/') } catch { continue }
    if ($full.StartsWith("$norm\", [StringComparison]::OrdinalIgnoreCase)) { return $true }
  }
  return $false
}

# Test seam: source the pure decisions without process or registry side effects.
if ($env:REGENT_TEST_UNINSTALL_LIB_ONLY) { return }

# 1) Stop the Regent processes OF THIS INSTALL (fine if none are running — also
#    while the app/CLI is mid-run, so the binaries below aren't locked).
#    "Regent" is the desktop app (Regent.exe); it spawns the deacon, so it goes
#    first. Anything running from a different install is left alone.
$ownRoots = @($binDir, $homeDir)
$stopped = @()
foreach ($name in "Regent", "regent-deacon", "regent-gateway", "regent-voice-server", "regent-cli") {
  Get-Process -Name $name -ErrorAction SilentlyContinue | ForEach-Object {
    $path = try { $_.Path } catch { $null }
    if (Test-RegentProcessOwned $path $ownRoots) {
      Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
      Write-Host "-> stopped $name (pid $($_.Id))"
      $stopped += $_
    } else {
      Write-Host "-> left $name (pid $($_.Id)) running - it belongs to another Regent install"
    }
  }
}
# Stop-Process returns before Windows has released the image handle, so the
# delete below raced it: "Access to the path 'regent-deacon.exe' is denied",
# and the script announced "removed" anyway. Wait for the exits we asked for.
if ($stopped.Count -gt 0) {
  $stopped | Wait-Process -Timeout 15 -ErrorAction SilentlyContinue
}
Get-ChildItem -Path $homeDir -Filter "*.pid" -ErrorAction SilentlyContinue |
  Remove-Item -Force -ErrorAction SilentlyContinue

# 2) Remove binaries + shim. Report what ACTUALLY happened: a locked binary
#    (something still running, an antivirus holding the file) left the directory
#    in place while the script printed "removed", so the next thing the user
#    knew was a half-uninstall behaving oddly.
if (Test-Path $binDir) {
  Remove-Item -Recurse -Force $binDir -ErrorAction SilentlyContinue
  if (Test-Path $binDir) {
    Write-Host "could not fully remove $binDir - close any running Regent and re-run this script"
  } else {
    Write-Host "removed $binDir"
  }
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

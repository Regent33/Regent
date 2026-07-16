# Regent one-line installer (Windows PowerShell):
#   irm https://raw.githubusercontent.com/Regent33/Regent/main/scripts/install.ps1 | iex
# Downloads the latest GitHub release into %USERPROFILE%\.regent\bin and puts
# `regent` on your PATH. Override the repo with $env:REGENT_REPO = "owner/repo".
$ErrorActionPreference = "Stop"

$repo = if ($env:REGENT_REPO) { $env:REGENT_REPO } else { "Regent33/Regent" }
$binDir = if ($env:REGENT_BIN_DIR) { $env:REGENT_BIN_DIR } else { Join-Path $env:USERPROFILE ".regent\bin" }

New-Item -ItemType Directory -Force $binDir | Out-Null

# Unzip without wildcard semantics. Expand-Archive globs its -DestinationPath
# and offers no -LiteralPath for it, so an install directory containing [ or ]
# fails with a bogus "already exists"; .NET treats the path literally, and is
# markedly faster on an archive this size.
# Entry by entry, because ExtractToDirectory's 3rd argument is an Encoding on
# .NET Framework (Windows PowerShell 5.1) — the bool-overwrite overload is .NET
# Core only — and the 2-argument form throws on reinstall. ExtractToFile does
# take an overwrite flag on both.
function Expand-Zip($zip, $dest) {
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  $archive = [System.IO.Compression.ZipFile]::OpenRead($zip)
  try {
    foreach ($entry in $archive.Entries) {
      if (-not $entry.Name) { continue }  # directory marker
      $out = Join-Path $dest $entry.FullName
      $parent = Split-Path -Parent $out
      if (-not (Test-Path -LiteralPath $parent)) {
        New-Item -ItemType Directory -Force $parent | Out-Null
      }
      [System.IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $out, $true)
    }
  } finally { $archive.Dispose() }
}

# Offline path: the GUI installer bundles the release archive and points us at
# it via REGENT_LOCAL_ARCHIVE, so no network or download is needed.
if ($env:REGENT_LOCAL_ARCHIVE -and (Test-Path -LiteralPath $env:REGENT_LOCAL_ARCHIVE)) {
  Write-Host "-> installing from local archive (offline): $env:REGENT_LOCAL_ARCHIVE"
  Expand-Zip $env:REGENT_LOCAL_ARCHIVE $binDir
} else {
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

  if (-not $fromSource) {
    Expand-Zip $tmp $binDir
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
}

# Shim + user PATH (the CLI finds regent-deacon as a sibling binary in binDir).
$shim = Join-Path $binDir "regent.cmd"
# -LiteralPath is load-bearing: -Encoding is a dynamic parameter contributed by
# the FileSystem provider, and a bin dir containing [ ] stops PowerShell
# resolving which provider the path belongs to — so -Encoding silently ceases to
# exist and the bind fails.
"@echo off`r`n`"$binDir\regent-cli.exe`" %*" |
  Set-Content -LiteralPath $shim -Encoding ascii

# Prepend $binDir to the user PATH without damaging it.
#
# The obvious [Environment]::GetEnvironmentVariable('Path','User') EXPANDS any
# %VAR% it finds, and SetEnvironmentVariable writes the result back as REG_SZ.
# A read-modify-write therefore bakes every %VAR% in someone's PATH into
# whatever it happened to mean today, and permanently downgrades the key from
# REG_EXPAND_SZ so later ones stop expanding too. Read the raw value straight
# from the registry instead and put the same type back.
function Add-UserPath($dir) {
  $key = Get-Item 'HKCU:\Environment'
  $raw = $key.GetValue('Path', '', 'DoNotExpandEnvironmentNames')
  $kind = try { $key.GetValueKind('Path') } catch { 'ExpandString' }
  if (($raw -split ';' | Where-Object { $_ }) -contains $dir) { return $false }
  $new = if ($raw) { "$dir;$raw" } else { $dir }
  Set-ItemProperty 'HKCU:\Environment' -Name Path -Value $new -Type $kind
  # SetEnvironmentVariable broadcasts WM_SETTINGCHANGE for you; a raw registry
  # write does not, and without it Explorer keeps handing new shells its cached
  # copy until the next sign-out.
  if (-not ('Regent.Env' -as [type])) {
    Add-Type -Namespace Regent -Name Env -MemberDefinition @'
[DllImport("user32.dll", SetLastError=true, CharSet=CharSet.Auto)]
public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam,
  string lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);
'@
  }
  $out = [UIntPtr]::Zero
  # HWND_BROADCAST, WM_SETTINGCHANGE, SMTO_ABORTIFHUNG, 5s — a hung window must
  # not hang the install.
  [void][Regent.Env]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero,
    'Environment', 2, 5000, [ref]$out)
  return $true
}

# REGENT_NO_PATH lets the GUI installer honour an unticked "add to PATH".
if (-not $env:REGENT_NO_PATH) {
  if (Add-UserPath $binDir) {
    Write-Host "added $binDir to your user PATH (open a new terminal to pick it up)"
  }
}

Write-Host "installed to $binDir"
Write-Host "Next: just run 'regent' - setup walks you through it on first launch."

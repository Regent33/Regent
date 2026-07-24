# Regent one-line installer (Windows PowerShell):
#   irm https://raw.githubusercontent.com/Regent33/Regent/main/scripts/install.ps1 | iex
# Downloads the latest GitHub release into %USERPROFILE%\.regent\bin and puts
# `regent` on your PATH. Override the repo with $env:REGENT_REPO = "owner/repo".
$ErrorActionPreference = "Stop"

$repo = if ($env:REGENT_REPO) { $env:REGENT_REPO } else { "Regent33/Regent" }
$homeDir = if ($env:REGENT_HOME) { $env:REGENT_HOME } else { Join-Path $env:USERPROFILE ".regent" }
$binDir = if ($env:REGENT_BIN_DIR) { $env:REGENT_BIN_DIR } else { Join-Path $homeDir "bin" }

New-Item -ItemType Directory -Force $binDir | Out-Null

# Literal, overwrite-safe extraction with zip-slip rejection.
function Expand-Zip($zip, $dest) {
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  $root = [IO.Path]::GetFullPath($dest).TrimEnd('\') + '\'
  $archive = [System.IO.Compression.ZipFile]::OpenRead($zip)
  try {
    foreach ($entry in $archive.Entries) {
      if (-not $entry.Name) { continue }  # directory marker
      $out = [IO.Path]::GetFullPath((Join-Path $dest $entry.FullName))
      if (-not $out.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) {
        throw "archive entry escapes install directory: $($entry.FullName)"
      }
      $parent = Split-Path -Parent $out
      if (-not (Test-Path -LiteralPath $parent)) {
        New-Item -ItemType Directory -Force $parent | Out-Null
      }
      [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $out, $true)
    }
  } finally { $archive.Dispose() }
}

# GUI installer offline payload: no network download.
if ($env:REGENT_LOCAL_ARCHIVE -and (Test-Path -LiteralPath $env:REGENT_LOCAL_ARCHIVE)) {
  Write-Host "-> installing from local archive (offline): $env:REGENT_LOCAL_ARCHIVE"
  Expand-Zip $env:REGENT_LOCAL_ARCHIVE $binDir
} else {
  $arch = if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq "Arm64") { "aarch64" } else { "x86_64" }
  $asset = "regent-windows-$arch.zip"
  $url = "https://github.com/$repo/releases/latest/download/$asset"

  Write-Host "-> downloading $asset from $repo (latest release)..."
  $tmp = Join-Path $env:TEMP "regent-install.zip"
  $shaTmp = "$tmp.sha256"
  $legacySha = if ($repo -eq "Regent33/Regent" -and $asset -eq "regent-windows-x86_64.zip") {
    "4EC1ECB32239E8B8C47FDF07844125317DF49AF6FD7C0B2A6758E94BBA76B859"
  } else { $null }
  $expected = $null
  $fromSource = $false
  try { Invoke-WebRequest -Uri $url -OutFile $tmp -UseBasicParsing }
  catch { $fromSource = $true }

  if (-not $fromSource) {
    try { Invoke-WebRequest -Uri "$url.sha256" -OutFile $shaTmp -UseBasicParsing }
    catch {
      if ($legacySha) {
        $expected = $legacySha
        Write-Host "using the pinned v0.1.1 checksum for $asset"
      } else {
        Write-Host "no checksum published for $asset - refusing the unverified archive"
        $fromSource = $true
      }
    }
  }
  if (-not $fromSource) {
    if (-not $expected) {
      $expected = ((Get-Content -LiteralPath $shaTmp -Raw) -split '\s+' | Where-Object { $_ })[0]
    }
    $actual = (Get-FileHash -LiteralPath $tmp -Algorithm SHA256).Hash
    if ($expected -notmatch '^[0-9a-fA-F]{64}$' -or $actual -ne $expected) {
      Write-Host "checksum verification failed for $asset - refusing the archive"
      $fromSource = $true
    } else {
      Write-Host "verified $asset (sha256)"
    }
  }
  Remove-Item $shaTmp -Force -ErrorAction SilentlyContinue

  if (-not $fromSource) {
    Expand-Zip $tmp $binDir
    Remove-Item $tmp -Force
  } else {
    Remove-Item $tmp -Force -ErrorAction SilentlyContinue
    Write-Host "no verified release for windows-$arch - building from source instead"
    foreach ($t in @(@('git', 'https://git-scm.com'), @('cargo', 'https://rustup.rs'), @('bun', 'https://bun.sh'))) {
      if (-not (Get-Command $t[0] -ErrorAction SilentlyContinue)) { Write-Host "need $($t[0]): $($t[1])"; exit 1 }
    }
    $src = if ($env:REGENT_SRC_DIR) { $env:REGENT_SRC_DIR } else { Join-Path $homeDir "src" }
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

# Optional ffmpeg: version/hash pinned; failure is non-fatal.
$FFMPEG_VERSION = "8.1.2"
$FFMPEG_SHA256 = "DB580001CAA24AC104C8CB856CD113A87B0A443F7BDF47D8C12B1D740584A2EC"
function Install-Ffmpeg($binDir) {
  if ($env:REGENT_NO_FFMPEG) { return }
  $target = Join-Path $binDir "ffmpeg.exe"
  if (Test-Path -LiteralPath $target) { return }
  if (Get-Command ffmpeg -ErrorAction SilentlyContinue) { return }
  $url = "https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-$FFMPEG_VERSION-essentials_build.zip"
  $tmp = Join-Path $env:TEMP "regent-ffmpeg.zip"
  try {
    Write-Host "-> fetching ffmpeg $FFMPEG_VERSION for camera capture (optional)..."
    Invoke-WebRequest -Uri $url -OutFile $tmp -UseBasicParsing
    $got = (Get-FileHash -LiteralPath $tmp -Algorithm SHA256).Hash
    if ($got -ne $FFMPEG_SHA256) {
      Write-Host "   (ffmpeg checksum mismatch - skipping; camera will prompt to install it later)"
      return
    }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [System.IO.Compression.ZipFile]::OpenRead($tmp)
    try {
      # The essentials build carries one ffmpeg.exe under <ver>/bin/ — extract
      # just that, not the whole archive (ffprobe/ffplay/docs aren't needed).
      $entry = $zip.Entries | Where-Object { $_.Name -eq "ffmpeg.exe" } | Select-Object -First 1
      if ($entry) {
        [System.IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $target, $true)
        Write-Host "   camera ready (ffmpeg -> $target)"
      }
    } finally { $zip.Dispose() }
  } catch {
    Write-Host "   (couldn't fetch ffmpeg; camera will prompt to install it when first used)"
  } finally {
    if (Test-Path -LiteralPath $tmp) { Remove-Item $tmp -Force -ErrorAction SilentlyContinue }
  }
}
Install-Ffmpeg $binDir

# Shim + user PATH (the CLI finds regent-deacon as a sibling binary in binDir).
$shim = Join-Path $binDir "regent.cmd"
# -LiteralPath keeps bracketed install paths on the FileSystem provider.
"@echo off`r`n`"$binDir\regent-cli.exe`" %*" |
  Set-Content -LiteralPath $shim -Encoding ascii

# Preserve raw %VAR% entries and the existing REG_SZ/REG_EXPAND_SZ type when
# prepending $binDir; the Environment APIs would expand and downgrade the value.
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

# When a person is driving an interactive install, run first-time setup for them
# now rather than leaving it for the next launch. Skipped for the GUI / offline
# embedding (REGENT_LOCAL_ARCHIVE), when the caller opts out (REGENT_NO_LAUNCH),
# and when no console is attached (a non-interactive `irm | iex` in CI must not
# block on a wizard).
function Invoke-Setup($binDir) {
  if ($env:REGENT_LOCAL_ARCHIVE -or $env:REGENT_NO_LAUNCH) { return }
  if (-not [Environment]::UserInteractive) { return }
  try { if ([Console]::IsInputRedirected) { return } } catch { }
  $cli = Join-Path $binDir "regent-cli.exe"
  if (-not (Test-Path -LiteralPath $cli)) { return }
  Write-Host ""
  Write-Host "-> starting first-time setup..."
  & $cli setup
}

Write-Host "If setup does not open here, run 'regent' to start it."
Invoke-Setup $binDir

# Hermetic checks for the real Windows installer/uninstaller production paths.
# Scratch files plus a mocked Invoke-WebRequest keep every case offline; PATH,
# HKCU, the user's home, and Regent processes are never touched.
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$installPs1 = Join-Path $root 'scripts\install.ps1'
$uninstallPs1 = Join-Path $root 'scripts\uninstall.ps1'
$script:pass = 0
$script:fail = 0
function Ok($cond, $msg, $detail) {
  if ($cond) { $script:pass++; Write-Host "  PASS $msg" }
  else {
    $script:fail++
    Write-Host "  FAIL $msg"
    if ($detail) { $detail | ForEach-Object { Write-Host "       | $_" } }
    # Name the failing check in the job summary, and carry the evidence with
    # it. Without this the only public signal from a red run is "Process
    # completed with exit code 1", and the raw log needs a repo-scoped token to
    # read — which turns a CI failure into guesswork about which case broke.
    # `$detail` exists because knowing WHICH check failed still left the actual
    # output invisible; %0A is how one annotation carries several lines.
    if ($env:GITHUB_ACTIONS) {
      $extra = if ($detail) { '%0A' + (($detail -join '%0A') -replace '\r', '') } else { '' }
      Write-Host "::error title=verify-install::$msg$extra"
    }
  }
}

$work = Join-Path ([IO.Path]::GetTempPath()) ('regent-verify-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force $work | Out-Null
try {
  $payload = Join-Path $work 'payload'
  New-Item -ItemType Directory -Force $payload | Out-Null
  Set-Content -LiteralPath (Join-Path $payload 'regent-cli.exe') -Value 'stub-cli' -Encoding ascii
  Set-Content -LiteralPath (Join-Path $payload 'regent-deacon.exe') -Value 'stub-deacon' -Encoding ascii
  $archive = Join-Path $work 'regent-windows-x86_64.zip'
  Compress-Archive -Path (Join-Path $payload '*') -DestinationPath $archive -Force
  $goodHash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash
  $goodSha = Join-Path $work 'good.sha256'
  $badSha = Join-Path $work 'bad.sha256'
  Set-Content -LiteralPath $goodSha -Value "$goodHash  fixture.zip" -Encoding ascii
  Set-Content -LiteralPath $badSha -Value "$('0' * 64)  fixture.zip" -Encoding ascii

  # This child defines only the external web/build commands, then runs the real
  # install.ps1 download branch. A refused archive reaches the blocked source
  # fallback and exits non-zero without contacting the network.
  $networkHarness = Join-Path $work 'network-harness.ps1'
  @'
param($Installer, $Archive, $Checksum, $Mode, $BinDir, $Scratch)
$ErrorActionPreference = 'Stop'
$env:TEMP = $Scratch
$env:REGENT_REPO = 'tests/Regent'
$env:REGENT_BIN_DIR = $BinDir
$env:REGENT_SRC_DIR = Join-Path $Scratch 'src'
$env:REGENT_NO_PATH = '1'
$env:REGENT_NO_FFMPEG = '1'
$env:REGENT_NO_LAUNCH = '1'
function Invoke-WebRequest {
  param($Uri, $OutFile, [switch]$UseBasicParsing)
  if ($Uri -like '*.sha256') {
    if ($Mode -eq 'missing') { throw 'fixture checksum missing' }
    Copy-Item -LiteralPath $Checksum -Destination $OutFile -Force
  } else { Copy-Item -LiteralPath $Archive -Destination $OutFile -Force }
}
function git { throw 'source fallback blocked by installer test' }
function cargo { throw 'source fallback blocked by installer test' }
function bun { throw 'source fallback blocked by installer test' }
& $Installer
'@ | Set-Content -LiteralPath $networkHarness -Encoding ascii

  function Invoke-InstallerChild($childArgs) {
    $oldPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
      $out = & powershell @childArgs 2>&1
      [pscustomobject]@{ Code = $LASTEXITCODE; Out = ($out -join "`n") }
    } finally { $ErrorActionPreference = $oldPreference }
  }
  function Invoke-NetworkInstall($mode, $binDir, $checksum) {
    $scratch = Join-Path $work "network-$mode"
    New-Item -ItemType Directory -Force $scratch | Out-Null
    Invoke-InstallerChild @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
      $networkHarness, '-Installer', $installPs1, '-Archive', $archive,
      '-Checksum', $checksum, '-Mode', $mode, '-BinDir', $binDir,
      '-Scratch', $scratch)
  }
  function Invoke-OfflineInstall($binDir, $sourceArchive, $homeDir) {
    $env:REGENT_LOCAL_ARCHIVE = $sourceArchive
    if ($binDir) { $env:REGENT_BIN_DIR = $binDir } else { Remove-Item Env:REGENT_BIN_DIR -ErrorAction SilentlyContinue }
    if ($homeDir) { $env:REGENT_HOME = $homeDir } else { Remove-Item Env:REGENT_HOME -ErrorAction SilentlyContinue }
    $env:REGENT_NO_PATH = '1'
    $env:REGENT_NO_FFMPEG = '1'
    $env:REGENT_NO_LAUNCH = '1'
    try {
      Invoke-InstallerChild @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $installPs1)
    } finally {
      Remove-Item Env:REGENT_LOCAL_ARCHIVE, Env:REGENT_BIN_DIR, Env:REGENT_HOME,
        Env:REGENT_NO_PATH, Env:REGENT_NO_FFMPEG, Env:REGENT_NO_LAUNCH -ErrorAction SilentlyContinue
    }
  }
  function Test-Installed($binDir) {
    (Test-Path -LiteralPath (Join-Path $binDir 'regent-cli.exe') -PathType Leaf) -and
    (Test-Path -LiteralPath (Join-Path $binDir 'regent-deacon.exe') -PathType Leaf) -and
    (Test-Path -LiteralPath (Join-Path $binDir 'regent.cmd') -PathType Leaf)
  }

  Write-Host 'install.ps1 - verified download branch'
  $goodBin = Join-Path $work 'good\bin'
  $r = Invoke-NetworkInstall 'good' $goodBin $goodSha
  Ok ($r.Code -eq 0) 'valid checksum exits 0'
  Ok (Test-Installed $goodBin) 'valid archive is extracted'
  Ok ($r.Out -match 'verified .*sha256') 'verification is reported'
  foreach ($mode in @('mismatch', 'missing')) {
    $bin = Join-Path $work "$mode\bin"
    $r = Invoke-NetworkInstall $mode $bin $badSha
    Ok ($r.Code -ne 0) "$mode checksum refuses the install"
    Ok (-not (Test-Path -LiteralPath (Join-Path $bin 'regent-cli.exe'))) "$mode checksum never extracts the archive"
    Ok ($r.Out -match 'refusing') "$mode refusal is explicit"
  }

  Write-Host 'install.ps1 - offline GUI payload / reinstall'
  $offlineBin = Join-Path $work 'offline\bin'
  $r = Invoke-OfflineInstall $offlineBin $archive
  Ok ($r.Code -eq 0) 'offline payload exits 0'
  Ok (Test-Installed $offlineBin) 'offline payload is extracted'
  Ok ($r.Out -match 'offline') 'offline branch is reported'
  $r = Invoke-OfflineInstall $offlineBin $archive
  Ok ($r.Code -eq 0) 'reinstall over the same directory exits 0'
  Ok (Test-Installed $offlineBin) 'reinstall leaves binaries intact'
  $customHome = Join-Path $work 'custom-home'
  $r = Invoke-OfflineInstall $null $archive $customHome
  Ok ($r.Code -eq 0) 'REGENT_HOME install exits 0'
  Ok (Test-Installed (Join-Path $customHome 'bin')) 'REGENT_HOME owns the default bin directory'

  Write-Host 'install.ps1 - quoted Unicode path'
  $unicodeName = 'R' + [char]0x00E9 + 'gent ' + [char]0x6F22 + " dir o'brien"
  $unicodeBin = Join-Path (Join-Path $work $unicodeName) 'bin'
  $r = Invoke-OfflineInstall $unicodeBin $archive
  Ok ($r.Code -eq 0) 'Unicode/spaced/quoted path exits 0'
  Ok (Test-Installed $unicodeBin) 'Unicode/spaced/quoted path receives binaries'

  Write-Host 'install.ps1 - archive path containment'
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  $badArchive = Join-Path $work 'traversal.zip'
  $zip = [IO.Compression.ZipFile]::Open($badArchive, [IO.Compression.ZipArchiveMode]::Create)
  try {
    $entry = $zip.CreateEntry('../escape.txt')
    $writer = New-Object IO.StreamWriter($entry.Open())
    try { $writer.Write('escape') } finally { $writer.Dispose() }
  } finally { $zip.Dispose() }
  $badBin = Join-Path $work 'traversal\bin'
  $escape = Join-Path (Split-Path -Parent $badBin) 'escape.txt'
  $r = Invoke-OfflineInstall $badBin $badArchive
  Ok ($r.Code -ne 0) 'path-traversal archive is rejected'
  Ok (-not (Test-Path -LiteralPath $escape)) 'path-traversal entry is not written'
  Ok ($r.Out -match 'escapes install directory') 'path-traversal error is actionable'

  Write-Host 'uninstall.ps1 - REGENT_DEACON_PATH pin decision'
  $oldPreference = $ErrorActionPreference
  $env:REGENT_TEST_UNINSTALL_LIB_ONLY = '1'
  try { . $uninstallPs1 } finally {
    Remove-Item Env:REGENT_TEST_UNINSTALL_LIB_ONLY -ErrorAction SilentlyContinue
    $ErrorActionPreference = $oldPreference
  }
  $pinBin = Join-Path $work 'pin\bin'
  $otherBin = Join-Path $work 'pin\other\bin'
  New-Item -ItemType Directory -Force $pinBin, $otherBin | Out-Null
  $otherDeacon = Join-Path $otherBin 'regent-deacon.exe'
  Set-Content -LiteralPath $otherDeacon -Value 'x' -Encoding ascii
  $insidePin = Join-Path $pinBin 'regent-deacon.exe'
  $fwdPin = $pinBin.Replace([char]92, [char]47) + '/regent-deacon.exe'
  $gonePin = Join-Path $work 'pin\gone\regent-deacon.exe'
  Ok (Test-RegentPinRemovable $insidePin $pinBin) 'pin inside the removed bin is cleared'
  Ok (Test-RegentPinRemovable $fwdPin $pinBin) 'forward-slash pin inside the bin is cleared'
  Ok (Test-RegentPinRemovable $gonePin $pinBin) 'pin whose target is gone is cleared'
  Ok (-not (Test-RegentPinRemovable $otherDeacon $pinBin)) 'valid different-install pin is kept'
  $viaParent = Join-Path $pinBin '..\other\bin\regent-deacon.exe'
  Ok (-not (Test-RegentPinRemovable $viaParent $pinBin)) 'normalized different-install pin is kept'
  $env:REGENT_TEST_PIN_ROOT = $pinBin
  try {
    Ok (Test-RegentPinRemovable '%REGENT_TEST_PIN_ROOT%\regent-deacon.exe' $pinBin) 'expanded pin inside the bin is cleared'
  } finally { Remove-Item Env:REGENT_TEST_PIN_ROOT -ErrorAction SilentlyContinue }
  Ok (Test-RegentPinRemovable $otherBin $pinBin) 'directory is not accepted as a valid binary pin'
  Ok (-not (Test-RegentPinRemovable '' $pinBin)) 'empty pin is ignored'
  Ok (-not (Test-RegentPinRemovable $null $pinBin)) 'null pin is ignored'

  # Which running processes this uninstall may stop. Matching on NAME alone
  # meant removing one install stopped another install's daemons — observed
  # for real: a sandbox uninstall killed the regent-voice-server of the
  # install in %LOCALAPPDATA%.
  Write-Host 'uninstall.ps1 - process ownership'
  $roots = @($pinBin, (Split-Path -Parent $pinBin))
  # A SEPARATE tree: $otherDeacon lives under $work\pin\other\bin, which is
  # inside this install's home, so it genuinely is ours. A second install lives
  # somewhere else entirely.
  $elsewhereBin = Join-Path $work 'elsewhere\bin'
  New-Item -ItemType Directory -Force $elsewhereBin | Out-Null
  $elsewhereDeacon = Join-Path $elsewhereBin 'regent-deacon.exe'
  Set-Content -LiteralPath $elsewhereDeacon -Value 'x' -Encoding ascii
  Ok (Test-RegentProcessOwned (Join-Path $pinBin 'regent-deacon.exe') $roots) `
    'a process running from the removed bin is stopped'
  Ok (Test-RegentProcessOwned (Join-Path (Split-Path -Parent $pinBin) 'Regent.exe') $roots) `
    'a process in the install root is stopped'
  Ok (-not (Test-RegentProcessOwned $elsewhereDeacon $roots)) `
    'another install''s process is left running'
  Ok (Test-RegentProcessOwned $otherDeacon $roots) `
    'a nested tree under this home is still ours'
  Ok (Test-RegentProcessOwned ($pinBin.Replace([char]92, [char]47) + '/regent-cli.exe') $roots) `
    'forward slashes still resolve to this install'
  Ok (Test-RegentProcessOwned (Join-Path $pinBin '..\bin\regent-gateway.exe') $roots) `
    'a path through .. normalizes into this install'
  Ok (Test-RegentProcessOwned $null $roots) `
    'an unreadable image path is stopped rather than left holding a lock'
  Ok (-not (Test-RegentProcessOwned 'C:\Windows\System32\notepad.exe' $roots)) `
    'an unrelated program is never stopped'

  # A locked binary used to leave the directory in place while the script still
  # announced "removed" — the user's next clue was a half-uninstall behaving
  # oddly. The whole script runs here; PATH and the pin are untouched because
  # this bin dir was never on PATH and any real pin lives elsewhere.
  Write-Host 'uninstall.ps1 - honest removal reporting'
  $lockHome = Join-Path $work 'locked'
  $lockBin = Join-Path $lockHome 'bin'
  New-Item -ItemType Directory -Force $lockBin | Out-Null
  $lockedFile = Join-Path $lockBin 'regent-cli.exe'
  Set-Content -LiteralPath $lockedFile -Value 'x' -Encoding ascii
  # NOT Out-String: it re-wraps at the host's console width, which is wide here
  # and 80 on a CI runner, so a long path split mid-word and the assertions
  # below matched different text on the two machines. Join the records instead
  # and compare whole lines against the literal path — no regex, so a GUID or a
  # bracket in the temp path cannot change the meaning either.
  $runUninstall = {
    param($home_, $bin)
    @(& powershell -NoProfile -ExecutionPolicy Bypass -Command `
        "`$env:REGENT_HOME='$home_'; `$env:REGENT_BIN_DIR='$bin'; & '$uninstallPs1'" 2>&1) |
      ForEach-Object { $_.ToString() }
  }
  $stream = [IO.File]::Open($lockedFile, 'Open', 'Read', 'None')
  try {
    $lines = & $runUninstall $lockHome $lockBin
    Ok ([bool]($lines | Where-Object { $_ -like "could not fully remove $lockBin*" })) `
      'a locked binary is reported, not silently skipped' $lines
    Ok (-not ($lines | Where-Object { $_ -like "removed $lockBin" })) `
      'a failed delete never claims the directory was removed' $lines
    Ok (Test-Path $lockedFile) 'the locked file is still there to explain the message' $lines
  } finally { $stream.Dispose() }
  $lines2 = & $runUninstall $lockHome $lockBin
  Ok ([bool]($lines2 | Where-Object { $_ -like "removed $lockBin" })) `
    'once unlocked, the same run removes it and says so' $lines2
  Ok (-not (Test-Path $lockBin)) 'the bin directory is really gone the second time'
  Ok (Test-Path $lockHome) 'data outside bin survives an uninstall without --purge'
} finally {
  Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}

Write-Host ''
Write-Host ('verify-install.ps1: {0} passed, {1} failed' -f $script:pass, $script:fail)
if ($script:fail -gt 0) { exit 1 }

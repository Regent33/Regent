# Ships the Visual C++ runtime next to the Windows binaries, then proves the
# package resolves every VC++ import it makes.
#   scripts/stage-vc-runtime.ps1 -Dest pkg
#
# regent-deacon.exe (statically linked ONNX Runtime) and onnxruntime.dll import
# msvcp140.dll / msvcp140_1.dll / vcruntime140*.dll - the VC++ Redistributable.
# A dev box and the windows-latest runner both have it, so every "does it
# start" probe passed; a fresh laptop does not, and the deacon died at load
# with "MSVCP140_1.dll was not found". App-local deployment (the CRT folder
# copied beside the exe) is Microsoft's supported alternative to vc_redist.exe
# and needs no elevation, so it fits a per-user install.
param(
  [Parameter(Mandatory)][string]$Dest,
  [string]$Arch = 'x64'
)
$ErrorActionPreference = 'Stop'

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (-not (Test-Path -LiteralPath $vswhere)) { throw "vswhere.exe not found - is Visual Studio / Build Tools installed?" }
$vs = & $vswhere -latest -products * -property installationPath
$crt = Get-ChildItem -LiteralPath (Join-Path $vs 'VC\Redist\MSVC') -Directory |
  Sort-Object Name -Descending |
  ForEach-Object { Join-Path $_.FullName "$Arch\Microsoft.VC143.CRT" } |
  Where-Object { Test-Path -LiteralPath $_ } |
  Select-Object -First 1
if (-not $crt) { throw "no $Arch\Microsoft.VC143.CRT under $vs\VC\Redist\MSVC" }

$dlls = Get-ChildItem -LiteralPath $crt -Filter *.dll
Copy-Item -LiteralPath $dlls.FullName -Destination $Dest -Force
Write-Host ("staged VC++ runtime from {0}: {1}" -f $crt, ($dlls.Name -join ', '))

# Closure check: every msvcp*/vcruntime* name mentioned by an exe/dll in $Dest
# must be a file in $Dest. Import tables are plain ASCII, so a byte scan is
# enough - no dumpbin dependency.
$present = @(Get-ChildItem -LiteralPath $Dest -Filter *.dll | ForEach-Object { $_.Name.ToLower() })
$missing = @()
foreach ($bin in Get-ChildItem -LiteralPath $Dest -File | Where-Object { $_.Extension -in '.exe', '.dll' }) {
  $text = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($bin.FullName))
  foreach ($m in [regex]::Matches($text, '(?i)\b(msvcp|vcruntime)[0-9a-z_]*\.dll')) {
    $name = $m.Value.ToLower()
    if ($present -notcontains $name) { $missing += "$($bin.Name) -> $name" }
  }
}
$missing = $missing | Sort-Object -Unique
if ($missing) {
  Write-Host "::error::packaged binaries import VC++ runtime DLLs that are not in the package (a machine without the VC++ Redistributable cannot start them):"
  $missing | ForEach-Object { Write-Host "::error::  $_" }
  exit 1
}
Write-Host "VC++ runtime closure verified for $Dest"

# Build release and stage files for Windows (folder + optional Inno Setup installer).
# Run from PowerShell in the repo root:
#   .\scripts\build-windows.ps1
# Optional: compile installer if Inno Setup is installed:
#   .\scripts\build-windows.ps1 -Installer
param(
    [switch]$Installer
)

$ErrorActionPreference = "Stop"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $Root

$versionLine = Select-String -Path (Join-Path $Root "Cargo.toml") -Pattern '^version = ' | Select-Object -First 1
if (-not $versionLine) { throw "Could not read version from Cargo.toml" }
$Version = [regex]::Match($versionLine.Line, '"([^"]+)"').Groups[1].Value

Write-Host "==> Building release (Windows)…" -ForegroundColor Cyan
cargo build --release --bin sortwise
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$exe = Join-Path $Root "target\release\sortwise.exe"
if (-not (Test-Path $exe)) { throw "Missing $exe" }

$outDir = Join-Path $Root "dist\Sortwise-Windows-v$Version"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
Copy-Item -Force $exe $outDir

$readme = @"
Sortwise (Windows) v$Version
=====================================

Run: double-click sortwise.exe

Suggested install:
  1. Move this folder to e.g. C:\Program Files\Sortwise\
  2. Pin or create a desktop shortcut to sortwise.exe

If SmartScreen warns about an unknown publisher, click "More info" then "Run anyway"
(for builds that are not code-signed).

For an .exe installer, install Inno Setup from https://jrsoftware.org/isinfo.php
and compile packaging\windows\Sortwise.iss (or run this script with -Installer).
"@
Set-Content -Path (Join-Path $outDir "README.txt") -Value $readme -Encoding UTF8

Write-Host ""
Write-Host "Staged: $outDir" -ForegroundColor Green

if ($Installer) {
    $iscc = @(
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "${env:ProgramFiles}\Inno Setup 6\ISCC.exe"
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1

    if (-not $iscc) {
        Write-Warning "Inno Setup 6 not found. Install from https://jrsoftware.org/isinfo.php"
        exit 0
    }

    $iss = Join-Path $Root "packaging\windows\Sortwise.iss"
    Write-Host "==> Compiling installer with Inno Setup…" -ForegroundColor Cyan
    & $iscc "/DMyAppVersion=$Version" $iss
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    Write-Host "Installer output under dist\" -ForegroundColor Green
}

Write-Host ""
Write-Host "Done." -ForegroundColor Green

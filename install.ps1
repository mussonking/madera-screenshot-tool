# Madera.SS Windows build helper
#
# Builds the production app and prints the generated installer paths.
# Pass -RunInstaller to launch the NSIS installer after a successful build.

param(
    [switch]$RunInstaller
)

Set-Location $PSScriptRoot

Write-Host "Building Madera.SS..." -ForegroundColor Cyan
npm run tauri build

if ($LASTEXITCODE -ne 0) {
    Write-Host "Build failed." -ForegroundColor Red
    exit $LASTEXITCODE
}

$BundleDir = Join-Path $PSScriptRoot "src-tauri\target\release\bundle"
$Setup = Get-ChildItem $BundleDir -Recurse -Filter "Madera.SS_*_x64-setup.exe" |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
$Msi = Get-ChildItem $BundleDir -Recurse -Filter "Madera.SS_*_x64_en-US.msi" |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1

Write-Host ""
Write-Host "Build complete." -ForegroundColor Green
if ($Setup) { Write-Host "NSIS installer: $($Setup.FullName)" -ForegroundColor Yellow }
if ($Msi) { Write-Host "MSI installer:  $($Msi.FullName)" -ForegroundColor Yellow }

if ($RunInstaller -and $Setup) {
    Write-Host ""
    Write-Host "Launching installer..." -ForegroundColor Cyan
    Start-Process -FilePath $Setup.FullName
}

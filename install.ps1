# Madera.Tools Installation Script
# Builds the app and creates Start Menu shortcut

Write-Host "Building Madera.Tools..." -ForegroundColor Cyan

# Build release version
Set-Location "$PSScriptRoot\src-tauri"
cargo build --release

if ($LASTEXITCODE -ne 0) {
    Write-Host "Build failed!" -ForegroundColor Red
    exit 1
}

Write-Host "Build successful!" -ForegroundColor Green

# Create Start Menu shortcut
$WshShell = New-Object -ComObject WScript.Shell
$ShortcutPath = "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Madera.Tools.lnk"
$Shortcut = $WshShell.CreateShortcut($ShortcutPath)
$Shortcut.TargetPath = "$PSScriptRoot\src-tauri\target\release\screenshot-tool.exe"
$Shortcut.WorkingDirectory = "$PSScriptRoot\src-tauri\target\release"
$Shortcut.Description = "Madera Tools - Screenshot, Color Picker, History, Desktop Guardian"
$Shortcut.IconLocation = "$PSScriptRoot\src-tauri\target\release\screenshot-tool.exe,0"
$Shortcut.Save()

Write-Host "Start Menu shortcut created!" -ForegroundColor Green
Write-Host ""
Write-Host "Installation complete! You can now launch 'Madera.Tools' from the Start Menu" -ForegroundColor Yellow
Write-Host ""
Write-Host "To start the app now, run:" -ForegroundColor Cyan
Write-Host "  .\src-tauri\target\release\screenshot-tool.exe" -ForegroundColor White

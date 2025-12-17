@echo off
set PATH=%PATH%;%USERPROFILE%\.cargo\bin
cd /d "%~dp0"
npm run tauri dev

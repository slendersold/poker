@echo off
setlocal
cd /d "%~dp0"
cargo build --release
if errorlevel 1 exit /b 1
if not exist dist mkdir dist
copy /Y "target\release\office_holdem.exe" "dist\OfficeHoldem.exe" >nul
echo Ready: dist\OfficeHoldem.exe


@echo off
rem ============================================================================
rem LiuZX SKF Service - one-click install (self-elevating)
rem Usage: right-click -> "Run as administrator"
rem ============================================================================
setlocal
set "SCRIPT_DIR=%~dp0"
set "POWERSHELL=%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"
"%POWERSHELL%" -NoProfile -ExecutionPolicy Bypass -Command "Start-Process '%POWERSHELL%' -ArgumentList '-NoProfile -ExecutionPolicy Bypass -File ""%SCRIPT_DIR%install.ps1""' -Verb RunAs"
endlocal

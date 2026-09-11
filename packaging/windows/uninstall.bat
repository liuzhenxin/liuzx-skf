@echo off
rem ============================================================================
rem LiuZX SKF Service - one-click uninstall (self-elevating)
rem ============================================================================
setlocal
set "SCRIPT_DIR=%~dp0"
set "POWERSHELL=%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"
"%POWERSHELL%" -NoProfile -ExecutionPolicy Bypass -Command "Start-Process '%POWERSHELL%' -ArgumentList '-NoProfile -ExecutionPolicy Bypass -File ""%SCRIPT_DIR%uninstall.ps1""' -Verb RunAs"
endlocal

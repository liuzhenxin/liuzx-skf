@echo off
setlocal
cd /d "%~dp0"

if not exist "skf-service.exe" (
  echo [ERROR] skf-service.exe not found in %CD%
  pause
  exit /b 1
)
if not exist "config\skf.yaml" (
  echo [ERROR] config\skf.yaml not found in %CD%
  pause
  exit /b 1
)
if not exist "native\GM3000\windows\mtoken_gm3000.dll" (
  echo [ERROR] GM3000 DLL not found: native\GM3000\windows\mtoken_gm3000.dll
  pause
  exit /b 1
)

set "RUST_LOG=info"
echo Starting LiuZX SKF Service in foreground mode...
echo WebSocket: ws://127.0.0.1:9001
echo HTTP Demo: http://127.0.0.1:8000/skf_api.html
echo Press Ctrl+C to stop.
echo.
"%~dp0skf-service.exe"

if errorlevel 1 (
  echo.
  echo [ERROR] SKF Service exited with code %ERRORLEVEL%.
  pause
)
endlocal

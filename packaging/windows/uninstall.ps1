# ============================================================================
# Uninstall LiuZX SKF Service (stop, unregister, remove files).
# Run from an elevated shell, or right-click uninstall.bat ->
# "Run as administrator".
# ============================================================================
#requires -RunAsAdministrator
$ErrorActionPreference = "Continue"

$ServiceName = "LiuZXSKFService"
$ProgramFilesX86 = ${env:ProgramFiles(x86)}
if ([string]::IsNullOrWhiteSpace($ProgramFilesX86)) {
    $ProgramFilesX86 = $env:ProgramFiles
}
$InstallDir  = Join-Path $ProgramFilesX86 "LiuZX\SKF Service"
$Exe         = Join-Path $InstallDir "skf-service.exe"

Write-Host "==> LiuZX SKF Service uninstaller =="

# Prefer the exe's own uninstall (stops + deletes the SCM service).
if ((Test-Path $Exe) -and (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue)) {
    & $Exe uninstall
}
# Fall back to sc.exe in case the payload was already removed.
if (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue) {
    sc.exe stop $ServiceName   | Out-Null
    sc.exe delete $ServiceName | Out-Null
    Start-Sleep -Milliseconds 800
}

if (Test-Path $InstallDir) {
    Remove-Item $InstallDir -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "Service '$ServiceName' removed. Install dir '$InstallDir' deleted."

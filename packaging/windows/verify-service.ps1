# ============================================================================
# End-to-end verification of the LiuZX SKF Service install lifecycle.
#
# Run on a Windows host as administrator:
#   powershell -ExecutionPolicy Bypass -File packaging\windows\verify-service.ps1
#
# Covers SVC-01..SVC-05 as far as a script can:
#   install -> status(Running/stage=serve) -> upgrade over running
#   -> ACL check -> uninstall -> assert no leftovers -> reinstall -> uninstall
#
# This is the Windows human-UAT artefact; it cannot run in the macOS/Linux test
# job. Use -KeepInstalled to leave the final install in place for manual
# observation of StartPending in `sc query`.
# ============================================================================
#requires -RunAsAdministrator
param([switch]$KeepInstalled)

$ErrorActionPreference = "Stop"

$ServiceName = "LiuZXSKFService"
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProgramFilesX86 = ${env:ProgramFiles(x86)}
if ([string]::IsNullOrWhiteSpace($ProgramFilesX86)) {
    $ProgramFilesX86 = $env:ProgramFiles
}
$InstallDir = Join-Path $ProgramFilesX86 "LiuZX\SKF Service"
$Exe        = Join-Path $InstallDir "skf-service.exe"

function Fail([string]$Message) {
    throw "FAIL: $Message"
}

function Assert-ServiceRunning {
    $svc = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
    if (-not $svc) { Fail "service '$ServiceName' is not registered" }
    if ($svc.Status -ne 'Running') { Fail "service '$ServiceName' is $($svc.Status), expected Running" }
}

function Assert-StatusStage([string]$Expected) {
    if (-not (Test-Path $Exe)) { Fail "skf-service.exe missing at $Exe" }
    $out = & $Exe status | Out-String
    if ($out -notmatch [regex]::Escape($Expected)) {
        Fail "status output did not contain '$Expected':`n$out"
    }
}

function Invoke-Installer {
    & (Join-Path $Here 'install.ps1')
    if ($LASTEXITCODE -ne 0) { Fail "install.ps1 exited $LASTEXITCODE" }
}

Write-Host "==> Step 1: install"
Invoke-Installer
Assert-ServiceRunning
Write-Host "PASS: install and service Running"

Write-Host "==> Step 2: status reports the serve phase (SVC-05)"
Assert-StatusStage "stage=serve"
Write-Host "PASS: status reports stage=serve"

Write-Host "==> Step 3: upgrade over a running installation (SVC-04)"
Invoke-Installer
Assert-ServiceRunning
Assert-StatusStage "stage=serve"
Write-Host "PASS: upgrade over running installation"

Write-Host "==> Step 4: install directory ACL (SVC-03)"
$usersLines = (icacls "$InstallDir") -split "`r?`n" | Where-Object { $_ -match 'Users' }
if ($usersLines -match '\(F\)|\(M\)|\(W\)') {
    Fail "install directory is writable by Users: $usersLines"
}
Write-Host "PASS: Users cannot write to the install directory"

Write-Host "==> Step 5: uninstall leaves no residue (SVC-04)"
& (Join-Path $Here 'uninstall.ps1')
if (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue) {
    Fail "service '$ServiceName' is still registered after uninstall"
}
if (Test-Path $InstallDir) {
    Fail "install directory still exists after uninstall: $InstallDir"
}
Write-Host "PASS: uninstall removed the registration and the directory"

Write-Host "==> Step 6: reinstall after uninstall (SVC-04)"
Invoke-Installer
Assert-ServiceRunning
Write-Host "PASS: reinstall succeeded"

if ($KeepInstalled) {
    Write-Host "==> -KeepInstalled set; leaving the service installed for manual inspection"
    Write-Host "ALL CHECKS PASSED (service left installed)"
    exit 0
}

Write-Host "==> Step 7: final cleanup"
& (Join-Path $Here 'uninstall.ps1')
Write-Host "ALL CHECKS PASSED"

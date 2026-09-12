# ============================================================================
# Uninstall LiuZX SKF Service (stop, unregister, remove files).
# Run from an elevated shell, or right-click uninstall.bat ->
# "Run as administrator".
#
# Waits for the process to exit and release its files before deleting the
# install directory, and fails loudly if cleanup does not succeed (SVC-04).
# ============================================================================
#requires -RunAsAdministrator
$ErrorActionPreference = "Stop"

$ServiceName = "LiuZXSKFService"
$ProgramFilesX86 = ${env:ProgramFiles(x86)}
if ([string]::IsNullOrWhiteSpace($ProgramFilesX86)) {
    $ProgramFilesX86 = $env:ProgramFiles
}
$InstallDir  = Join-Path $ProgramFilesX86 "LiuZX\SKF Service"
$Exe         = Join-Path $InstallDir "skf-service.exe"

function Get-ServiceOrNull {
    param([string]$Name)
    try { return Get-Service -Name $Name -ErrorAction Stop }
    catch { return $null }
}

function Wait-ServiceStoppedAndUnlocked {
    param(
        [string]$ServiceName,
        [string]$ExePath,
        [int]$TimeoutSeconds = 30
    )
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        $svc = Get-ServiceOrNull -Name $ServiceName
        $isStopped = (-not $svc) -or ($svc.Status -eq 'Stopped')

        $isUnlocked = $true
        if (Test-Path $ExePath) {
            try {
                $fs = [IO.File]::Open($ExePath, 'Open', 'ReadWrite', 'None')
                $fs.Close()
            } catch {
                $isUnlocked = $false
            }
        }

        if ($isStopped -and $isUnlocked) { return $true }
        Start-Sleep -Milliseconds 300
    }
    return $false
}

Write-Host "==> LiuZX SKF Service uninstaller =="

# Prefer the exe's own uninstall (stops + deletes the SCM service).
if ((Test-Path $Exe) -and (Get-ServiceOrNull -Name $ServiceName)) {
    & $Exe uninstall
}
# Fall back to sc.exe in case the payload was already removed.
if (Get-ServiceOrNull -Name $ServiceName) {
    sc.exe stop $ServiceName   | Out-Null
    sc.exe delete $ServiceName | Out-Null
}

if (-not (Wait-ServiceStoppedAndUnlocked -ServiceName $ServiceName -ExePath $Exe)) {
    throw "service '$ServiceName' did not stop or release '$Exe' within 30s; refusing to delete files"
}

if (Get-ServiceOrNull -Name $ServiceName) {
    throw "service '$ServiceName' is still registered after delete"
}

if (Test-Path $InstallDir) {
    Remove-Item $InstallDir -Recurse -Force -ErrorAction Stop
}

if (Test-Path $InstallDir) {
    throw "install directory still exists after removal: $InstallDir"
}

Write-Host "Service '$ServiceName' removed. Install dir '$InstallDir' deleted."

# ============================================================================
# Install LiuZX SKF Service as a native Windows service (AutoStart).
# Run from an elevated shell, or simply right-click install.bat and choose
# "Run as administrator".
#
# Safe to run over a running installation: it stops the old process, waits for
# the executable to be released, replaces the payload, then re-registers and
# starts the service (SVC-04).
# ============================================================================
#requires -RunAsAdministrator
$ErrorActionPreference = "Stop"

$ServiceName = "LiuZXSKFService"
$SourceDir   = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProgramFilesX86 = ${env:ProgramFiles(x86)}
if ([string]::IsNullOrWhiteSpace($ProgramFilesX86)) {
    $ProgramFilesX86 = $env:ProgramFiles
}
$InstallDir  = Join-Path $ProgramFilesX86 "LiuZX\SKF Service"
$Exe         = Join-Path $InstallDir "skf-service.exe"

# --- helpers ----------------------------------------------------------------

# Wait until the service is Stopped and its executable can be opened
# exclusively. A fixed sleep is not enough: the process may still hold the file
# while the SCM has already marked it Stopped.
function Wait-ServiceStoppedAndUnlocked {
    param(
        [string]$ServiceName,
        [string]$ExePath,
        [int]$TimeoutSeconds = 30
    )
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        $svc = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
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

Write-Host "==> LiuZX SKF Service installer =="
Write-Host "    Install dir : $InstallDir"
Write-Host "    Service name: $ServiceName (AutoStart)"

# --- 1. Stop and remove any previous service + payload (idempotent install) --
if (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue) {
    Write-Host "==> Stopping previous service ..."
    sc.exe stop $ServiceName   | Out-Null
    sc.exe delete $ServiceName | Out-Null
}
if (Test-Path $InstallDir) {
    if (-not (Wait-ServiceStoppedAndUnlocked -ServiceName $ServiceName -ExePath $Exe)) {
        throw "service '$ServiceName' did not stop or release '$Exe' within 30s; refusing to replace files"
    }
    Write-Host "==> Removing previous payload ..."
    Remove-Item $InstallDir -Recurse -Force -ErrorAction Stop
}
else {
    # Nothing installed; still make sure a stale registration is gone.
    $null = Wait-ServiceStoppedAndUnlocked -ServiceName $ServiceName -ExePath $Exe
}

# --- 2. Copy payload --------------------------------------------------------
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item -Path (Join-Path $SourceDir "*") -Destination $InstallDir -Recurse -Force

if (-not (Test-Path $Exe)) {
    throw "skf-service.exe not found after copy: $Exe"
}

# --- 3. Restrict the install directory (SVC-03) -----------------------------
# Only SYSTEM and Administrators may write; Users get read+execute. This stops a
# non-administrator replacing the loaded executable or the vendor DLL.
Write-Host "==> Applying restrictive ACL to $InstallDir ..."
& icacls "$InstallDir" /inheritance:r `
    /grant:r "SYSTEM:(OI)(CI)F" `
    /grant:r "Administrators:(OI)(CI)F" `
    /grant:r "Users:(OI)(CI)RX" | Out-Null
if ($LASTEXITCODE -ne 0) { throw "icacls failed with exit code $LASTEXITCODE" }

$usersLines = (icacls "$InstallDir") -split "`r?`n" | Where-Object { $_ -match 'Users' }
if ($usersLines -match '\(F\)|\(M\)|\(W\)') {
    throw "install directory is still writable by Users: $usersLines"
}

# --- 4. Register the service with the SCM and start it ----------------------
# skf-service.exe install:
#   - registers "LiuZXSKFService" (LocalSystem, OWN_PROCESS, AutoStart)
#   - the service image is launched by the SCM with the "--service" argument
#   - starts the service immediately (no reboot needed)
Write-Host "==> Registering and starting Windows service ..."
& $Exe install
if ($LASTEXITCODE -ne 0) { throw "skf-service.exe install failed" }

# --- 5. Auto-restart on failure (crash recovery) ----------------------------
# The service exits with a distinct non-zero code on startup failure, which makes
# this action fire (SVC-02).
& sc.exe failure $ServiceName reset= 86400 actions= restart/5000/restart/10000/restart/60000 | Out-Null

# --- 6. Verify --------------------------------------------------------------
Write-Host ""
& $Exe status
Write-Host ""
Write-Host "Done. Manage via: services.msc  /  sc query $ServiceName"
Write-Host "Logs: $InstallDir\skf-service.log"

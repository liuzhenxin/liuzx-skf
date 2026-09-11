# ============================================================================
# Install LiuZX SKF Service as a native Windows service (AutoStart).
# Run from an elevated shell, or simply right-click install.bat and choose
# "Run as administrator".
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

Write-Host "==> LiuZX SKF Service installer =="
Write-Host "    Install dir : $InstallDir"
Write-Host "    Service name: $ServiceName (AutoStart)"

# --- 1. Remove any previous service + payload (idempotent install) ----------
if (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue) {
    Write-Host "==> Removing previous service registration ..."
    sc.exe stop $ServiceName   | Out-Null
    sc.exe delete $ServiceName | Out-Null
    Start-Sleep -Milliseconds 800
}
if (Test-Path $InstallDir) {
    Remove-Item $InstallDir -Recurse -Force -ErrorAction SilentlyContinue
}

# --- 2. Copy payload --------------------------------------------------------
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item -Path (Join-Path $SourceDir "*") -Destination $InstallDir -Recurse -Force

if (-not (Test-Path $Exe)) {
    throw "skf-service.exe not found after copy: $Exe"
}

# --- 3. Register the service with the SCM and start it ----------------------
# skf-service.exe install:
#   - registers "LiuZXSKFService" (LocalSystem, OWN_PROCESS, AutoStart)
#   - the service image is launched by the SCM with the "--service" argument
#   - starts the service immediately (no reboot needed)
Write-Host "==> Registering and starting Windows service ..."
& $Exe install
if ($LASTEXITCODE -ne 0) { throw "skf-service.exe install failed" }

# --- 4. Auto-restart on failure (crash recovery) ----------------------------
& sc.exe failure $ServiceName reset= 86400 actions= restart/5000/restart/10000/restart/60000 | Out-Null

# --- 5. Verify --------------------------------------------------------------
Write-Host ""
& $Exe status
Write-Host ""
Write-Host "Done. Manage via: services.msc  /  sc query $ServiceName"
Write-Host "Logs: $InstallDir\skf-service.log"

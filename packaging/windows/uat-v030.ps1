# ============================================================================
# One-shot Windows UAT for the v0.3.0 release (checklist section A + C2).
#
# Run in an ELEVATED PowerShell on a Windows host:
#   powershell -ExecutionPolicy Bypass -File packaging\windows\uat-v030.ps1
#
# What it does, in order:
#   C2  download the released ZIP + .sha256 and verify the hash independently
#   1/5/6/7  run verify-service.ps1 (install -> status -> upgrade -> ACL ->
#            uninstall -> reinstall) and summarise its PASS lines
#   3   install, break config, confirm a non-zero service exit code and a
#       failed(provider) status file, then restore the config
#   final uninstall so the host is left clean
#
# A2 (StartPending observation) is timing-sensitive and is left manual; see
# docs/PRE-RELEASE-UAT.md.
#
# This script orchestrates already-written installers/verifiers; it is a UAT aid,
# not part of the shipped service.
# ============================================================================
#requires -RunAsAdministrator
param([string]$ZipPath = "")

$ErrorActionPreference = "Stop"

$ServiceName = "LiuZXSKFService"
$ProgramFilesX86 = ${env:ProgramFiles(x86)}
if ([string]::IsNullOrWhiteSpace($ProgramFilesX86)) { $ProgramFilesX86 = $env:ProgramFiles }
$InstallDir = Join-Path $ProgramFilesX86 "LiuZX\SKF Service"
$ReleaseBase = "https://github.com/liuzhenxin/liuzx-skf/releases/download/v0.3.0/skf-service-windows-x64-gm3000-x86.zip"
$Work = Join-Path $env:TEMP "skf-uat-v030"
$Results = New-Object System.Collections.Generic.List[string]

function Record([string]$Id, [bool]$Ok, [string]$Detail) {
    $tag = if ($Ok) { "PASS" } else { "FAIL" }
    $Results.Add(("{0}`t{1}`t{2}" -f $Id, $tag, $Detail))
    Write-Host ("[{0}] {1} - {2}" -f $tag, $Id, $Detail)
}

function Step([string]$Id, [scriptblock]$Body) {
    try { & $Body; Record $Id $true "ok" }
    catch { Record $Id $false $_.Exception.Message }
}

if (Test-Path $Work) { Remove-Item $Work -Recurse -Force }
New-Item -ItemType Directory -Force -Path $Work | Out-Null

# --- C2: obtain and verify the released artifact ----------------------------
$zip = if ($ZipPath) { $ZipPath } else { Join-Path $Work "skf.zip" }
if (-not $ZipPath) {
    Invoke-WebRequest "$ReleaseBase" -OutFile $zip
    Invoke-WebRequest "$ReleaseBase.sha256" -OutFile "$zip.sha256"
}
if (Test-Path "$zip.sha256") {
    Step "C2" {
        $expected = ((Get-Content "$zip.sha256") -split ' ')[0].Trim().ToLower()
        $actual = (Get-FileHash -Algorithm SHA256 -Path $zip).Hash.ToLower()
        if ($expected -ne $actual) { throw "checksum mismatch: $expected vs $actual" }
    }
} else {
    Record "C2" $false "no .sha256 file next to $zip"
}

# --- A1/A5/A6/A7: lifecycle via the shipped verifier ------------------------
$pkg = Join-Path $Work "skf"
Expand-Archive -Path $zip -DestinationPath $pkg -Force
Set-ExecutionPolicy -Scope Process -Force Bypass
$verify = Join-Path $pkg "verify-service.ps1"
if (-not (Test-Path $verify)) { throw "verify-service.ps1 not found in the package" }

Step "A1/A5/A6/A7" {
    $out = & powershell -ExecutionPolicy Bypass -File $verify 2>&1 | Out-String
    Write-Host $out
    if ($out -notmatch "ALL CHECKS PASSED") { throw "verify-service.ps1 did not report success" }
    if ($out -notmatch "PASS: install and service Running") { throw "install/Running step failed" }
    if ($out -notmatch "PASS: Users cannot write") { throw "ACL step failed" }
    if ($out -notmatch "PASS: upgrade over running installation") { throw "upgrade step failed" }
    if ($out -notmatch "PASS: reinstall succeeded") { throw "reinstall step failed" }
}

# --- A3: invalid configuration is fatal -------------------------------------
Step "A3" {
    & (Join-Path $pkg "install.ps1") | Out-Host
    $cfg = Join-Path $InstallDir "config\skf.yaml"
    Copy-Item $cfg "$cfg.bak" -Force
    try {
        (Get-Content $cfg) -replace '^default:.*', 'default: NOPE' | Set-Content $cfg
        sc.exe stop $ServiceName | Out-Null; Start-Sleep 2
        sc.exe start $ServiceName | Out-Null; Start-Sleep 3
        $svc = Get-CimInstance Win32_Service -Filter "Name='$ServiceName'"
        $state = Get-Content (Join-Path $InstallDir "service-state.json") | Out-String
        Write-Host "service ExitCode=$($svc.ExitCode)"
        Write-Host $state
        if (-not $svc.ExitCode -or $svc.ExitCode -eq 0) { throw "service exited with code 0, restart would not fire" }
        if ($state -notmatch '"state":\s*"failed"') { throw "status file did not record a failure" }
        if ($state -notmatch '"stage":\s*"provider"') { throw "status file did not record stage=provider" }
        if ($state -notmatch '"code":\s*2') { throw "status file did not record code=2" }
    } finally {
        Move-Item "$cfg.bak" $cfg -Force
    }
}

# --- leave the host clean ---------------------------------------------------
Step "A7-cleanup" {
    & (Join-Path $pkg "uninstall.ps1") | Out-Host
    if (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue) { throw "service still registered" }
}

Write-Host ""
Write-Host "==================== UAT SUMMARY ===================="
$Results | ForEach-Object { Write-Host $_ }
$failed = ($Results | Where-Object { $_ -match "`tFAIL`t" }).Count
Write-Host "===================================================="
Write-Host ("Total: {0}  Failed: {1}" -f $Results.Count, $failed)
if ($failed -gt 0) { exit 1 }

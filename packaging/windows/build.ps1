# ============================================================================
# Build a standalone package for:
#   Windows x64 OS + GM3000 x86 SKF DLL + i686 skf-service.exe
#
# Run on Windows x64 with Rust and Visual Studio Build Tools installed:
#   powershell -ExecutionPolicy Bypass -File .\packaging\windows\build.ps1
# ============================================================================
param(
    [string]$Target = "i686-pc-windows-msvc"
)

$ErrorActionPreference = "Stop"

$Root       = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$ExeName    = "skf-service.exe"
$DistName   = "skf-service-windows-x64-gm3000-x86"
$Dist       = Join-Path $Root "dist\$DistName"
$Zip        = Join-Path $Root "dist\$DistName.zip"
$BuiltExe   = Join-Path $Root "target\$Target\release\$ExeName"
$VendorDll  = Join-Path $Root "native\GM3000\windows\mtoken_gm3000.dll"
$ConfigFile = Join-Path $PSScriptRoot "skf-windows-x86.yaml"

function Get-PeMachine([string]$Path) {
    $stream = [System.IO.File]::OpenRead($Path)
    $reader = [System.IO.BinaryReader]::new($stream)
    try {
        $stream.Seek(0x3C, [System.IO.SeekOrigin]::Begin) | Out-Null
        $peOffset = $reader.ReadInt32()
        $stream.Seek($peOffset + 4, [System.IO.SeekOrigin]::Begin) | Out-Null
        return $reader.ReadUInt16()
    } finally {
        $reader.Dispose()
        $stream.Dispose()
    }
}

if (-not (Test-Path $VendorDll)) {
    throw "GM3000 DLL not found: $VendorDll"
}
if ((Get-PeMachine $VendorDll) -ne 0x014C) {
    throw "GM3000 DLL is not x86 (expected PE Machine 0x014C): $VendorDll"
}

Push-Location $Root
$OldRustFlags = $env:RUSTFLAGS
try {
    Write-Host "==> Installing Rust target $Target ..."
    rustup target add $Target
    if ($LASTEXITCODE -ne 0) { throw "rustup target add failed" }

    # Static CRT avoids requiring a separate Visual C++ Redistributable for the
    # Rust executable. The GM3000 hardware driver must still be installed.
    $StaticCrt = "-C target-feature=+crt-static"
    if ([string]::IsNullOrWhiteSpace($OldRustFlags)) {
        $env:RUSTFLAGS = $StaticCrt
    } else {
        $env:RUSTFLAGS = "$OldRustFlags $StaticCrt"
    }

    Write-Host "==> Building $Target (release, static CRT) ..."
    cargo build --release --target $Target
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
} finally {
    $env:RUSTFLAGS = $OldRustFlags
    Pop-Location
}

if (-not (Test-Path $BuiltExe)) {
    throw "Expected binary not found: $BuiltExe"
}
if ((Get-PeMachine $BuiltExe) -ne 0x014C) {
    throw "Built executable is not x86 (expected PE Machine 0x014C): $BuiltExe"
}

Write-Host "==> Assembling standalone package ..."
if (Test-Path $Dist) { Remove-Item $Dist -Recurse -Force }
New-Item -ItemType Directory -Force -Path $Dist | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $Dist "config") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $Dist "native\GM3000\windows") | Out-Null

Copy-Item $BuiltExe  -Destination (Join-Path $Dist $ExeName) -Force
Copy-Item $ConfigFile -Destination (Join-Path $Dist "config\skf.yaml") -Force
Copy-Item $VendorDll -Destination (Join-Path $Dist "native\GM3000\windows\mtoken_gm3000.dll") -Force
Copy-Item (Join-Path $Root "api") -Destination $Dist -Recurse -Force

foreach ($Name in @(
    "run.bat",
    "install.bat",
    "install.ps1",
    "uninstall.bat",
    "uninstall.ps1",
    "verify-service.ps1",
    "uat-v030.ps1",
    "README-Windows-x86_64.md"
)) {
    Copy-Item (Join-Path $PSScriptRoot $Name) -Destination $Dist -Force
}

if (Test-Path $Zip) { Remove-Item $Zip -Force }
Compress-Archive -Path (Join-Path $Dist "*") -DestinationPath $Zip -CompressionLevel Optimal

# Publish an independently verifiable checksum next to the ZIP as
# <zip>.sha256 in sha256sum format.
$hash = (Get-FileHash -Algorithm SHA256 -Path $Zip).Hash.ToLower()
$sumFile = "$Zip.sha256"
"$hash *$(Split-Path -Leaf $Zip)" | Set-Content -Encoding ascii -Path $sumFile

Write-Host ""
Write-Host "Build complete:"
Write-Host "  Folder: $Dist"
Write-Host "  ZIP   : $Zip"
Write-Host "  SHA256: $sumFile"
Write-Host ""
Write-Host "Portable run : extract ZIP and double-click run.bat"
Write-Host "Install service: right-click install.bat and run as administrator"

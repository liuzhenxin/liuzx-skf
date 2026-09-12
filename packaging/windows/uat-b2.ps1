# ============================================================================
# B2 — device-removal invalidation (SVC-04b / SESS-04).
#
# Run in an ELEVATED PowerShell on a Windows host with the GM3000 driver and the
# token attached to the service. It is interactive: it asks you to physically
# remove and re-insert the token.
#
#   powershell -ExecutionPolicy Bypass -File packaging\windows\uat-b2.ps1
#
# Expected:
#   1. A CheckPIN succeeds and SignData works.
#   2. After removing the token, SignData fails (device not found).
#   3. After re-inserting, SignData is REFUSED with -10: the removal cleared the
#      grant, so CheckPIN is required again.
#
# Step 3 is the security-critical assertion. If it returns a signature instead,
# the grant survived removal and SESS-04b is not satisfied.
# ============================================================================
param(
    [string]$Pin = "",
    [string]$Provider = "",
    [string]$Url = "ws://127.0.0.1:9001"
)

$ErrorActionPreference = "Stop"

function New-Socket([string]$Target) {
    $ws = New-Object System.Net.WebSockets.ClientWebSocket
    $ws.ConnectAsync([Uri]$Target, [Threading.CancellationToken]::None).GetAwaiter().GetResult() | Out-Null
    return $ws
}
function Send-Only($ws, [string]$Method, $Params, [int]$Id) {
    $obj = @{ jsonrpc = "2.0"; method = $Method; params = $Params; id = $Id }
    $bytes = [Text.Encoding]::UTF8.GetBytes(($obj | ConvertTo-Json -Compress -Depth 8))
    $seg = [ArraySegment[byte]]::new($bytes)
    $ws.SendAsync($seg, [Net.WebSockets.WebSocketMessageType]::Text, $true, [Threading.CancellationToken]::None).GetAwaiter().GetResult() | Out-Null
}
function Receive-Rpc($ws) {
    $ms = New-Object System.IO.MemoryStream
    $buf = New-Object byte[] 65536
    do {
        $r = $ws.ReceiveAsync([ArraySegment[byte]]::new($buf), [Threading.CancellationToken]::None).GetAwaiter().GetResult()
        if ($r.MessageType -eq [Net.WebSockets.WebSocketMessageType]::Close) { throw "socket closed by server" }
        $ms.Write($buf, 0, $r.Count)
    } while (-not $r.EndOfMessage)
    return ([Text.Encoding]::UTF8.GetString($ms.ToArray()) | ConvertFrom-Json)
}
function Send-Rpc($ws, [string]$Method, $Params, [int]$Id) { Send-Only $ws $Method $Params $Id; return Receive-Rpc $ws }
function Brief($r) { return ($r | ConvertTo-Json -Compress -Depth 6) }

if ([string]::IsNullOrEmpty($Provider)) {
    $cfg = Join-Path ${env:ProgramFiles(x86)} "LiuZX\SKF Service\config\skf.yaml"
    $Provider = "GM3000"
    if (Test-Path $cfg) {
        $line = (Get-Content $cfg | Where-Object { $_ -match '^default:' } | Select-Object -First 1)
        if ($line) { $Provider = ($line -replace '^default:\s*', '').Trim('"', "'", ' ') }
    }
}
$secure = Read-Host -AsSecureString "Enter the token user PIN (hidden)"
$Pin = [Runtime.InteropServices.Marshal]::PtrToStringAuto([Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure))
if ([string]::IsNullOrEmpty($Pin)) { throw "a PIN is required" }

$ws = New-Socket $Url
$d = (Send-Rpc $ws "EnumDevice" @() 1).result
if (-not $d) { throw "no device detected; attach the token first" }
$device = @($d)[0]
$ap = (Send-Rpc $ws "EnumApplication" @($Provider, $device) 2).result
if (-not $ap) { throw "no application on '$device'" }
$app = @($ap)[0]
$cn = (Send-Rpc $ws "EnumContainer" @($Provider, $device, $app) 3).result
if (-not $cn) { throw "no container on '$device/$app'" }
$container = @($cn)[0]
$certKey = "$Provider/$device/$app/$container"
$digest = [Convert]::ToBase64String((New-Object byte[] 32))
Write-Host "device='$device' app='$app' container='$container'"
Write-Host ""

$pin = Send-Rpc $ws "CheckPIN" @("$Provider/$device/$app", $Pin) 10
if ($pin.error -ne 0) { throw "CheckPIN failed: $(Brief $pin)" }
$before = Send-Rpc $ws "SignData" @($certKey, $digest) 11
if ($before.error -ne 0) { throw "baseline SignData failed: $(Brief $before)" }
Write-Host "authorized and signing works."

Read-Host "PHYSICALLY REMOVE the token now, then press Enter"
Start-Sleep 2
$removed = Send-Rpc $ws "SignData" @($certKey, $digest) 12
Write-Host ("SignData while removed -> {0}" -f (Brief $removed))

Read-Host "RE-INSERT the token now, then press Enter"
Start-Sleep 3
$after = Send-Rpc $ws "SignData" @($certKey, $digest) 13
Write-Host ("SignData after re-insert -> {0}" -f (Brief $after))

Write-Host ""
if ($removed.error -eq 0) {
    Write-Host "[FAIL] B2 - SignData succeeded while the token was removed"
    exit 1
}
if ($after.error -eq -10) {
    Write-Host "[PASS] B2 - removal failed the operation and cleared the grant; re-insert required CheckPIN (-10)"
    exit 0
}
if ($after.error -eq 0) {
    Write-Host "[FAIL] B2 - the grant SURVIVED removal: SignData still succeeded after re-insert"
    Write-Host "       SESS-04b is not satisfied: a grant must be cleared when an operation finds the device gone."
    exit 1
}
Write-Host ("[WARN] B2 - inconclusive: after re-insert SignData returned {0} (not -10 and not success)" -f (Brief $after))
exit 2

# ============================================================================
# Real-token UAT for v0.3.0 (checklist section B, scriptable parts).
#
# Run in an ELEVATED PowerShell on a Windows host that has the GM3000 driver
# installed and the token attached to the service (not just the console session):
#
#   powershell -ExecutionPolicy Bypass -File packaging\windows\uat-token.ps1
#
# It asks for the user PIN, then runs:
#   B1  session isolation   : A CheckPIN succeeds, B SignData is refused (-10)
#   B4  concurrent signing  : two sessions sign at once; no crash, both succeed
#   B5  slow-op isolation   : SetLanguage stays responsive during FindCertificates
#
# It probes every container with a 32-byte digest first, because SignData signs a
# precomputed digest (SM2 expects 32 bytes) and some containers (e.g. `admin`) are
# not signing containers. B2/B6/B7/B8 are printed as manual steps.
#
# Uses only System.Net.WebSockets (no Node/Rust needed).
# ============================================================================
param(
    [string]$Pin = "",
    [string]$Provider = "",
    [string]$Url = "ws://127.0.0.1:9001"
)

$ErrorActionPreference = "Stop"
$Results = New-Object System.Collections.Generic.List[string]

function Record([string]$Id, [bool]$Ok, [string]$Detail) {
    $tag = if ($Ok) { "PASS" } else { "FAIL" }
    $Results.Add(("{0}`t{1}`t{2}" -f $Id, $tag, $Detail))
    Write-Host ("[{0}] {1} - {2}" -f $tag, $Id, $Detail)
}

function New-Socket([string]$Target) {
    $ws = New-Object System.Net.WebSockets.ClientWebSocket
    $ws.ConnectAsync([Uri]$Target, [Threading.CancellationToken]::None).GetAwaiter().GetResult() | Out-Null
    return $ws
}

function Send-Only($ws, [string]$Method, $Params, [int]$Id) {
    $obj = @{ jsonrpc = "2.0"; method = $Method; params = $Params; id = $Id }
    $json = $obj | ConvertTo-Json -Compress -Depth 8
    $bytes = [Text.Encoding]::UTF8.GetBytes($json)
    $seg = [ArraySegment[byte]]::new($bytes)
    $ws.SendAsync($seg, [Net.WebSockets.WebSocketMessageType]::Text, $true, [Threading.CancellationToken]::None).GetAwaiter().GetResult() | Out-Null
}

function Receive-Rpc($ws) {
    $ms = New-Object System.IO.MemoryStream
    $buf = New-Object byte[] 65536
    do {
        $seg = [ArraySegment[byte]]::new($buf)
        $r = $ws.ReceiveAsync($seg, [Threading.CancellationToken]::None).GetAwaiter().GetResult()
        if ($r.MessageType -eq [Net.WebSockets.WebSocketMessageType]::Close) { throw "socket closed by server" }
        $ms.Write($buf, 0, $r.Count)
    } while (-not $r.EndOfMessage)
    $text = [Text.Encoding]::UTF8.GetString($ms.ToArray())
    return ($text | ConvertFrom-Json)
}

function Send-Rpc($ws, [string]$Method, $Params, [int]$Id) {
    Send-Only $ws $Method $Params $Id
    return Receive-Rpc $ws
}

function Brief($response) { return ($response | ConvertTo-Json -Compress -Depth 6) }

# --- inputs -----------------------------------------------------------------
if ([string]::IsNullOrEmpty($Provider)) {
    $cfg = Join-Path ${env:ProgramFiles(x86)} "LiuZX\SKF Service\config\skf.yaml"
    $Provider = "GM3000"
    if (Test-Path $cfg) {
        $line = (Get-Content $cfg | Where-Object { $_ -match '^default:' } | Select-Object -First 1)
        if ($line) { $Provider = ($line -replace '^default:\s*', '').Trim('"', "'", ' ') }
    }
}
if ([string]::IsNullOrEmpty($Pin)) {
    $secure = Read-Host -AsSecureString "Enter the token user PIN (hidden)"
    $Pin = [Runtime.InteropServices.Marshal]::PtrToStringAuto(
        [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure))
}
if ([string]::IsNullOrEmpty($Pin)) { throw "a PIN is required" }

Write-Host "provider='$Provider'  url='$Url'"
Write-Host ""

# --- discover device/application/container ----------------------------------
$bootstrap = New-Socket $Url
$d = (Send-Rpc $bootstrap "EnumDevice" @() 1).result
if (-not $d) { throw "no device detected; is the token attached to the service?" }
$device = @($d)[0]
$ap = (Send-Rpc $bootstrap "EnumApplication" @($Provider, $device) 2).result
if (-not $ap) { throw "no application on device '$device'" }
$app = @($ap)[0]
$cn = (Send-Rpc $bootstrap "EnumContainer" @($Provider, $device, $app) 3).result
if (-not $cn) { throw "no container on '$device/$app'" }
$containers = @($cn)
Write-Host "device='$device' app='$app' containers=$($containers -join ', ')"
Write-Host ""

# --- authorize A and find a container that can sign -------------------------
# SignData signs a precomputed digest; SM2 expects 32 bytes.
$digest = [Convert]::ToBase64String((New-Object byte[] 32))
$firstKey = "$Provider/$device/$app/$($containers[0])"

$a = New-Socket $Url
$pinA = Send-Rpc $a "CheckPIN" @("$Provider/$device/$app", $Pin) 10
if ($pinA.error -ne 0) { throw "CheckPIN on A failed: $(Brief $pinA)" }

$signKey = $null
$probeId = 100
foreach ($c in $containers) {
    $key = "$Provider/$device/$app/$c"
    $type = Send-Rpc $a "GetContainerType" @($Provider, $device, $app, $c) $probeId; $probeId++
    $probe = Send-Rpc $a "SignData" @($key, $digest) $probeId; $probeId++
    Write-Host ("container '{0}' type={1} SignData -> {2}" -f $c, (Brief $type), (Brief $probe))
    if ($probe.error -eq 0) { $signKey = $key; Write-Host "selected signable container: $c"; break }
}
if (-not $signKey) {
    Write-Host "WARNING: no container accepted a 32-byte digest signature."
    Write-Host "         B4 will be reported as BLOCKED (container/input issue, not concurrency)."
}
Write-Host ""

# --- B1: session isolation --------------------------------------------------
try {
    $b = New-Socket $Url
    $bSign = Send-Rpc $b "SignData" @($firstKey, $digest) 11
    if ($bSign.error -ne -10) {
        throw "session B SignData must be refused with -10 without its own CheckPIN, got: $(Brief $bSign)"
    }
    Record "B1" $true "A authorized; B refused with -10 as expected"
} catch { Record "B1" $false $_.Exception.Message }

# --- B4: concurrent signing -------------------------------------------------
if (-not $signKey) {
    Record "B4" $false "BLOCKED: no signable container found (see probe output above)"
} else {
    try {
        $pinB = Send-Rpc $b "CheckPIN" @("$Provider/$device/$app", $Pin) 12
        if ($pinB.error -ne 0) { throw "CheckPIN on B failed: $(Brief $pinB)" }

        # Fire both without waiting; collect both regardless of outcome.
        Send-Only $a "SignData" @($signKey, $digest) 20
        Send-Only $b "SignData" @($signKey, $digest) 21
        $ra = Receive-Rpc $a
        $rb = Receive-Rpc $b
        Write-Host ("concurrent A -> {0}" -f (Brief $ra))
        Write-Host ("concurrent B -> {0}" -f (Brief $rb))
        if ($ra.error -ne 0) { throw "concurrent SignData A failed: $(Brief $ra)" }
        if ($rb.error -ne 0) { throw "concurrent SignData B failed: $(Brief $rb)" }
        if (-not $ra.result -or -not $rb.result) { throw "a concurrent signature was empty" }

        $alive = Send-Rpc $a "SetLanguage" @("EN") 22
        if ($alive.error -ne 0) { throw "service became unresponsive after concurrency: $(Brief $alive)" }
        Record "B4" $true "two overlapping SignData calls succeeded on '$signKey'; service still responsive"
    } catch { Record "B4" $false $_.Exception.Message }
}

# --- B5: a slow device operation must not stall another session -------------
try {
    Send-Only $a "FindCertificates" @("") 30
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $langResponse = Send-Rpc $b "SetLanguage" @("EN") 31
    $elapsed = $sw.Elapsed.TotalSeconds
    $slow = Receive-Rpc $a
    if ($langResponse.error -ne 0) { throw "SetLanguage failed during the slow operation" }
    if ($elapsed -ge 3.0) { throw "SetLanguage took ${elapsed}s while FindCertificates ran; the runtime is not isolated" }
    Record "B5" $true ("SetLanguage answered in {0:N2}s while FindCertificates was in flight" -f $elapsed)
} catch { Record "B5" $false $_.Exception.Message }

# --- cleanup ----------------------------------------------------------------
foreach ($s in @($a, $b, $bootstrap)) { try { $s.Dispose() } catch {} }

Write-Host ""
Write-Host "==================== TOKEN UAT SUMMARY ===================="
$Results | ForEach-Object { Write-Host $_ }
$failed = ($Results | Where-Object { $_ -match "`tFAIL`t" }).Count
Write-Host "=========================================================="
Write-Host ("Total: {0}  Failed: {1}" -f $Results.Count, $failed)
Write-Host ""
Write-Host "Manual steps still required (see docs/PRE-RELEASE-UAT.md):"
Write-Host "  B2  authorize A, physically remove the token, then call SignData on A:"
Write-Host "      expect device-not-found and the grant cleared (re-insert still needs CheckPIN)."
Write-Host "  B6  force a native error mid-operation (remove the token during a long op):"
Write-Host "      confirm the device/application/container are released (device usable again)."
Write-Host "  B7  a genuinely hung operation must return -1 'Device operation timed out after 30s'."
Write-Host "  B8  record any vendor concurrency documentation; it decides per-device vs per-provider lock."
if ($failed -gt 0) { exit 1 }

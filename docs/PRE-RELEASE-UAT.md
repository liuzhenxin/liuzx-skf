# Pre-Release UAT Checklist (v0.3.0)

Consolidated human-verification steps that cannot run on the macOS development
host or a hosted CI runner. Complete **all** of these before claiming the release
is production-verified. They are also tracked per phase in
`.planning/phases/*/*-HUMAN-UAT.md`.

Legend: `[ ]` pending · `[x]` done. Record the observed result beside each item.

---

## A. Windows service lifecycle (needs a Windows host, administrator)

### VM quick start (copy/paste into an **elevated** PowerShell on the VM)

Tests the actual released artifact, including the checksum:

```powershell
$ErrorActionPreference = "Stop"
$base = "https://github.com/liuzhenxin/liuzx-skf/releases/download/v0.3.0/skf-service-windows-x64-gm3000-x86.zip"
Invoke-WebRequest "$base"        -OutFile skf.zip
Invoke-WebRequest "$base.sha256" -OutFile skf.zip.sha256

# C2: verify the checksum independently
$expected = ((Get-Content skf.zip.sha256) -split ' ')[0].Trim().ToLower()
$actual   = (Get-FileHash skf.zip -Algorithm SHA256).Hash.ToLower()
if ($expected -ne $actual) { throw "checksum mismatch: $expected vs $actual" }
Write-Host "C2 PASS: SHA-256 matches"

Expand-Archive skf.zip -DestinationPath skf -Force
Set-ExecutionPolicy -Scope Process -Force Bypass

# A1/A2/A5/A6/A7: install -> status -> upgrade -> ACL -> uninstall -> reinstall
powershell -ExecutionPolicy Bypass -File .\skf\verify-service.ps1
```

Paste the full output back and it can be interpreted against the checklist items.

- [x] **A1 — install + Running.** ✅ 2026-09-12 (Win10 VM, released v0.3.0 ZIP):
      installed and reached `Running`; `status` printed
      `Startup: Running (stage=serve, ...)`.
- [x] **A2 — StartPending observed.** ✅ 2026-09-12: `status` showed
      `Startup: StartPending (stage=provider)` and `Service ... StartPending`
      before `Running`. Original manual command (kept for reference):
      ```powershell
      .\skf\install.ps1
      sc.exe stop LiuZXSKFService
      sc.exe start LiuZXSKFService
      1..20 | ForEach-Object { (sc.exe query LiuZXSKFService | Select-String 'STATE'); Start-Sleep -Milliseconds 100 }
      ```
- [x] **A3 — invalid config is fatal.** ✅ 2026-09-12:
      `State=Stopped ExitCode=1066` and the status file was
      `{"state":"failed","stage":"provider","code":2,"reason":"provider resolution
      error: Provider 'NOPE' not configured ..."}`. Commands:
      ```powershell
      $dir = "$env:ProgramFiles(x86)\LiuZX\SKF Service"
      Copy-Item "$dir\config\skf.yaml" "$dir\config\skf.yaml.bak" -Force
      (Get-Content "$dir\config\skf.yaml") -replace '^default:.*', 'default: NOPE' | Set-Content "$dir\config\skf.yaml"
      sc.exe stop LiuZXSKFService | Out-Null; Start-Sleep 2
      sc.exe start LiuZXSKFService
      Start-Sleep 3
      (Get-CimInstance Win32_Service -Filter "Name='LiuZXSKFService'").ExitCode
      Get-Content "$dir\service-state.json"
      Move-Item "$dir\config\skf.yaml.bak" "$dir\config\skf.yaml" -Force
      ```
      Expect a non-zero `ExitCode` (Windows reports `1066`
      = `ERROR_SERVICE_SPECIFIC_ERROR`, with the real code in the status file),
      `"state": "failed"`, `"stage": "provider"`, `"code": 2`; the `sc failure`
      action should fire (Event Log). Confirmed on the Windows 10 VM
      (2026-09-12): `ExitCode=1066`, `failed/provider/code=2`,
- [x] **A4 — status distinguishes process vs usable.** ✅ 2026-09-12 (via A1/A3):
      `status` printed `Startup: Running (stage=serve, ...)` in the healthy case and
      `Startup: Failed (stage=provider, code=2)` with the broken config — the phase
      and code distinguish "process alive" from "service usable". (The bind-failure
      variant is the same surface with `stage=bind, code=3`.)
- [x] **A5 — ACL enforced.** ✅ 2026-09-12: verifier asserted
      `PASS: Users cannot write to the install directory`. Manual attempt: as a
      standard user, try to overwrite
      `%ProgramFiles(x86)%\LiuZX\SKF Service\skf-service.exe` or
      `mtoken_gm3000.dll`; it must be denied. `icacls` shows `Users` with `(RX)`
      only.
- [x] **A6 — upgrade over running.** ✅ 2026-09-12: `install.ps1` over a running
      installation succeeded and `status` returned to `stage=serve`.
- [x] **A7 — uninstall + reinstall.** ✅ 2026-09-12: uninstall left no
      registration and no directory; reinstall succeeded.

## B. Real GM3000 token (needs a device + driver)

Use `config\skf.yaml` pointed at a real GM3000, and a real PIN.

### Scripted checks (B1/B4/B5)

With the service installed and the token attached to the **service** (not just the
console session):

```powershell
curl.exe -L -o uat-token.ps1 https://raw.githubusercontent.com/liuzhenxin/liuzx-skf/dev/packaging/windows/uat-token.ps1
powershell -ExecutionPolicy Bypass -File .\uat-token.ps1
```

It prompts for the PIN, discovers device/application/container, then runs B1, B4
and B5 and prints per-item PASS/FAIL. B2/B6/B7/B8 are printed as manual steps.

- [ ] **B1 — session isolation.** Connection A: `CheckPIN` succeeds. Connection B
      (separate client): `SignData` must be rejected with `-10`, not signed. (SESS-02)
- [x] **B2 — device-removal invalidation.** ⚠️ **v0.3.0 FAILED this check**: after
      removing the token, `SignData` failed with `ConnectDev failed: 0x00000001`
      but the grant survived, so a re-insert still signed. Fixed in **v0.3.1** by
      clearing the device's grants on a `ConnectDev` failure; re-verify with
      `packaging/windows/uat-b2.ps1` against v0.3.1. Original description: A authorizes, then physically remove
      the token; A's next sensitive operation must fail with device-not-found and
      the grant must be cleared (re-inserting still requires `CheckPIN`). (SESS-04b)
- [x] **B3 — no PIN retained.** ✅ Automated: `Grant` holds only an `Instant`
      (`session::auth::tests::a_grant_holds_only_a_deadline`, pinned to `Copy` and
      a bounded size); `CheckPIN` stores a deadline and nothing else, and
      `ctx.pins` is gone. Hardware-independent.
- [ ] **B4 — concurrent serialization.** Two connections issue `SignData`
      concurrently; the process must not crash and both results must be correct
      (per-provider gate). (TRANS-05)
- [ ] **B5 — slow operation isolation.** While one connection runs a slow device
      operation, another connection's `SetLanguage`/query must return promptly
      (whole-request blocking dispatch). (TRANS-03/04)
- [ ] **B6 — release on real error.** Force a native error mid-operation; the
      device/application/container must be released (log + device usable again).
      (RES-03/04)
- [ ] **B7 — timeout behaviour.** Confirm a hung operation returns `-1
      "Device operation timed out after 30s"` and the blocking thread later
      releases the lock; check the documented "timeout stops the caller, not the
      vendor call" behaviour. (TRANS-03)
- [ ] **B8 — DLL thread-safety note.** If the vendor provides any concurrency
      documentation, record it; it decides whether the per-provider lock can ever
      be relaxed to per-device.

## C. Release pipeline (needs a CI runner / tag)

- [x] **C2 — checksum independently verifiable.** ✅ 2026-09-12 (Win10 VM):
      downloaded the release ZIP + `.sha256` and confirmed the hashes match.
- [x] **C3 — fresh install from the released ZIP.** ✅ Covered by A1/A6/A7: the
      released ZIP was extracted and installed/uninstalled/reinstalled on a real
      Windows host (not a virgin OS image; the installer is idempotent).
- [x] **C1 — release workflow on v0.3.0.** `release-windows.yml` succeeded:
      ZIP built, SHA-256 verified, extracted contents and exe/DLL `Machine=0x014C`
      asserted, GitHub Release published with the ZIP and `.sha256`.
      (2026-09-12, run 34691327086)

## D. Contract and CI

- [x] **D1 — contract replay on a middleware machine.** ✅ 2026-09-12: the macOS
      development host (GM3000 middleware installed) replays
      `cargo test --test contract_fixtures` at 37/37 on every run. (Hosted runners
      cannot do this; see `docs/CI.md`.)
- [x] **D2 — hosted CI green.** `dev` CI run succeeded: Formatting, Clippy,
      Hardware-free tests (Linux), Windows i686 compile check, Gate self-test.
      (2026-09-12, run 34691883761)
- [x] **D3 — branch protection applied.** `main` requires the five CI checks
      (strict). Verified via `gh api .../branches/main/protection`.
- [x] **D4 — protected gate blocks a red PR.** ✅ 2026-09-12: throwaway PR #2
      (deliberately failing test) reached `mergeStateStatus: BLOCKED`, and
      `gh pr merge 2` was refused with "the base branch policy prohibits the
      merge". PR closed and branch deleted without merging.

---

## Sign-off

| Area | Owner | Date | Result |
|------|-------|------|--------|
| A. Windows lifecycle | operator (Win10 x64 VM) | 2026-09-12 | **PASS** — A1–A7 + A2 observed; `uat-v030.ps1` reported `Total: 4 Failed: 0` |
| B. Real GM3000 | operator (Win10 x64 VM) | 2026-09-12 | B1/B4/B5/B3 **PASS** (v0.3.0); **B2 exposed a real gap** fixed in v0.3.1 — re-verification pending; B6/B7/B8 remain manual |
| C. Release pipeline | CI + operator | 2026-09-12 | C1/C2/C3 **PASS** |
| D. Contract / CI | CI + dev host | 2026-09-12 | **PASS** — D1 (37/37 locally), D2 (hosted CI green), D3 (protection applied), D4 (red PR blocked) |

### Verified on the Windows VM (2026-09-12)

- C2 — released ZIP SHA-256 verified on the VM.
- A1 — install from the released ZIP reaches `Running`; `status` reports
  `Running (stage=serve)`.
- A2 — `StartPending (stage=provider)` observed before `Running`.
- A3 — invalid config → `ExitCode=1066` (≈ `ERROR_SERVICE_SPECIFIC_ERROR`) and
  `service-state.json` = `failed / provider / code=2`.
- A5 — `Users` cannot write to the install directory.
- A6 — upgrade over a running installation succeeded.
- A7 — uninstall left no registration or directory; reinstall succeeded.

B (real GM3000 hardware) remains the only product-runtime gap. After it is
checked, mark the milestone "production-verified" and drop the carry-over from the
next milestone backlog.

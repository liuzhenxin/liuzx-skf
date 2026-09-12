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

- [ ] **A1 — install + Running.** The script installs and the service reaches
      `Running`; `status` prints `Startup: Running (stage=serve, address=...)`.
- [ ] **A2 — StartPending observed.** Re-run with `-KeepInstalled`, then during
      startup observe `sc query LiuZXSKFService` pass through `START_PENDING`
      before `RUNNING`:
      ```powershell
      .\skf\install.ps1
      sc.exe stop LiuZXSKFService
      sc.exe start LiuZXSKFService
      1..20 | ForEach-Object { (sc.exe query LiuZXSKFService | Select-String 'STATE'); Start-Sleep -Milliseconds 100 }
      ```
- [ ] **A3 — invalid config is fatal.** Break `config\skf.yaml`, start the service,
      and confirm a non-zero exit code plus the `Failed` status file:
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
- [ ] **A4 — status distinguishes process vs usable.** Occupy port 9001, start the
      service, and confirm `status` shows `Failed (stage=bind, code=3)` while the
      SCM may briefly show the process differently.
- [ ] **A5 — ACL enforced.** As a standard user, attempt to overwrite
      `%ProgramFiles(x86)%\LiuZX\SKF Service\skf-service.exe` or
      `mtoken_gm3000.dll`; it must be denied. `icacls` shows `Users` with `(RX)`
      only.
- [ ] **A6 — upgrade over running.** While the service runs, run `install.ps1`
      again; it must stop, wait for the file to unlock, replace, restart, and
      `status` returns to `stage=serve`.
- [ ] **A7 — uninstall + reinstall.** `uninstall.ps1` removes the registration and
      the directory; re-running `install.ps1` succeeds.

## B. Real GM3000 token (needs a device + driver)

Use `config\skf.yaml` pointed at a real GM3000, and a real PIN.

- [ ] **B1 — session isolation.** Connection A: `CheckPIN` succeeds. Connection B
      (separate client): `SignData` must be rejected with `-10`, not signed. (SESS-02)
- [ ] **B2 — device-removal invalidation.** A authorizes, then physically remove
      the token; A's next sensitive operation must fail with device-not-found and
      the grant must be cleared (re-inserting still requires `CheckPIN`). (SESS-04b)
- [ ] **B3 — no PIN retained.** After `CheckPIN`, no code path re-reads a PIN; the
      grant is a deadline only. (SESS-05; covered structurally by
      `session::auth::tests::a_grant_holds_only_a_deadline`)
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

- [x] **C1 — release workflow on v0.3.0.** `release-windows.yml` succeeded:
      ZIP built, SHA-256 verified, extracted contents and exe/DLL `Machine=0x014C`
      asserted, GitHub Release published with the ZIP and `.sha256`.
      (2026-09-12, run 34691327086)
- [ ] **C2 — checksum independently verifiable.** Download the ZIP and
      `.sha256` from the release and verify:
      `sha256sum -c skf-service-windows-x64-gm3000-x86.zip.sha256`.
- [ ] **C3 — fresh install from the released ZIP.** On a clean Windows machine,
      run `install.bat`, exercise a token operation, then `uninstall.bat`.

## D. Contract and CI

- [ ] **D1 — contract replay on a middleware machine.** On a host with the GM3000
      macOS middleware installed: `cargo test --test contract_fixtures` → 37/37.
      (Hosted runners cannot do this; see `docs/CI.md`.)
- [x] **D2 — hosted CI green.** `dev` CI run succeeded: Formatting, Clippy,
      Hardware-free tests (Linux), Windows i686 compile check, Gate self-test.
      (2026-09-12, run 34691883761)
- [x] **D3 — branch protection applied.** `main` requires the five CI checks
      (strict). Verified via `gh api .../branches/main/protection`.
- [ ] **D4 — protected gate blocks a red PR.** Open a throwaway PR that fails a
      check and confirm merge is blocked. (The `gate-selftest` job proves the
      check command fails on a failing test; this step exercises the protection
      rule itself.)

---

## Sign-off

| Area | Owner | Date | Result |
|------|-------|------|--------|
| A. Windows lifecycle | | | |
| B. Real GM3000 | | | |
| C. Release pipeline | | | |
| D. Contract / CI | | | |

When A, B, C2/C3 and D1/D4 are all checked, update
`.planning/MILESTONES.md` / the audit with "production-verified" and remove the
corresponding carry-over items from the next milestone backlog.

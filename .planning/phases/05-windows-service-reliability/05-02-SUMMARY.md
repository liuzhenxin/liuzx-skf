---
phase: 05-windows-service-reliability
plan: 02
subsystem: packaging
tags: [svc-03, svc-04, installer, acl]

requires: [05-01]
provides:
  - restrictive install-directory ACL, applied and verified by install.ps1
  - poll-until-unlocked upgrade/uninstall
  - verify-service.ps1 Windows UAT script
  - docs/WINDOWS-SERVICE.md
affects: [phase-6]

tech-stack:
  added: []
  patterns:
    - "Wait on an observable condition (SCM Stopped + exclusive file open) instead of a fixed sleep"

key-files:
  created:
    - packaging/windows/verify-service.ps1
    - docs/WINDOWS-SERVICE.md
  modified:
    - packaging/windows/install.ps1
    - packaging/windows/uninstall.ps1

key-decisions:
  - "ACL: SYSTEM:F / Administrators:F / Users:RX with /inheritance:r, verified after apply"
  - "Upgrade/uninstall poll for up to 30s rather than sleeping"
  - "Uninstall fails loudly instead of swallowing cleanup errors"
  - "Service account stays LocalSystem"

requirements-completed: [SVC-03, SVC-04]

duration: 40min
completed: 2026-09-11
---

# Phase 5 Plan 2: Install Permissions and Reliable Upgrade/Uninstall Summary

**The installer now locks down its directory and refuses to replace locked files, and a Windows script exercises the whole install/upgrade/uninstall lifecycle.**

## Changes

| File | Change |
|------|--------|
| `install.ps1` | `icacls /inheritance:r` grant (SYSTEM:F, Administrators:F, Users:RX) + post-apply check that Users has no `(F)/(M)/(W)`; `Wait-ServiceStoppedAndUnlocked` replaces the 800 ms sleep; `Remove-Item -ErrorAction Stop`; restart actions unchanged |
| `uninstall.ps1` | same wait helper (no `SilentlyContinue`); strict `Remove-Item`; asserts the registration is gone |
| `verify-service.ps1` | install → status(`stage=serve`) → upgrade over running → ACL check → uninstall (no residue) → reinstall → uninstall; `PASS:` per step, `ALL CHECKS PASSED`, `-KeepInstalled` option |
| `docs/WINDOWS-SERVICE.md` | stages, status file, exit codes, fatal-vs-degraded boundary, install/upgrade/uninstall, ACL, verification |

## Evidence

| Check | Result |
|-------|--------|
| `grep -c 'icacls' install.ps1` | 3 |
| `grep -c '/inheritance:r' install.ps1` | 1 |
| `grep -c 'writable by Users' install.ps1` | 1 |
| `grep -c 'Milliseconds 800' install.ps1` | 0 |
| `grep -c 'Start-Sleep -Milliseconds 300' install.ps1` | 1 (polling helper only) |
| `grep -c 'Wait-ServiceStoppedAndUnlocked' install.ps1` | 3 (definition + 2 calls) |
| `grep -c 'SilentlyContinue' uninstall.ps1` | 0 |
| `test -f packaging/windows/verify-service.ps1` | true |
| `grep -c 'PASS:' verify-service.ps1` | 6 |
| `cargo test` | 142 passed, 0 failed |
| `cargo check --target i686-pc-windows-gnu` | exit 0 |

## Not yet executed (Windows only)

`verify-service.ps1` has **not been run**: this host is macOS and `pwsh` is not
installed, so the PowerShell scripts were written and cross-read but not executed.
The real SCM ordering, restart action, ACL enforcement, and file-lock behaviour are
recorded in `05-HUMAN-UAT.md` and require a Windows VM.

## Notes

- The ACL grant is applied with three `/grant:r` arguments rather than one combined
  string, which `icacls` accepts and which keeps the intent explicit.
- `verify-service.ps1` prints `ALL CHECKS PASSED` in two places (the `-KeepInstalled`
  path and the normal path); both are success messages.

## Self-Check: PASSED (automated portion)

- Scripts contain the required ACL, wait, and no-silent-failure logic
- `cargo test` 142/0 and i686 cross-check green
- Windows execution deferred to human UAT

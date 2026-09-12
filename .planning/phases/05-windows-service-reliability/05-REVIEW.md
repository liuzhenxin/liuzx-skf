---
phase: 05
phase_name: windows-service-reliability
status: clean
depth: standard
findings_total: 4
high: 0
medium: 1
low: 3
fixed: 0
reviewed_at: 2026-09-11
reviewer: inline (no subagent runtime available)
---

# Phase 5 Code Review

Scope: `src/service_state.rs`, `src/server/mod.rs` (startup classification),
`src/win_service.rs` (startup reporting, status), and the packaging scripts
(`install.ps1`, `uninstall.ps1`, `verify-service.ps1`, `docs/WINDOWS-SERVICE.md`).

## Findings

### M-1 (accepted, verification gap) — the PowerShell lifecycle is unexecuted

This host is macOS and `pwsh` is not installed, so `install.ps1`,
`uninstall.ps1`, and `verify-service.ps1` were written and cross-read but not run.
The real SCM ordering, the restart action, ACL enforcement, and file-lock
behaviour are therefore **unverified until a Windows VM run**. Recorded in
`05-HUMAN-UAT.md`. Everything platform-independent (state file, exit-code mapping,
startup classification) is unit-tested and green.

### L-1 (accepted) — `updated` is Unix epoch seconds

CONTEXT D-01 said ISO-8601. Formatting UTC without a date crate would be
hand-rolled, so the field holds epoch seconds. It is informational only and
documented in `docs/WINDOWS-SERVICE.md`.

### L-2 (accepted) — the upgrade wait checks the executable, not the vendor DLL

`Wait-ServiceStoppedAndUnlocked` proves `skf-service.exe` is released. The vendor
DLL is loaded by that process via `libloading`, so its release follows, but the
check does not open `mtoken_gm3000.dll` explicitly. If a deployment ever loads the
DLL out-of-process this would need to check it too. Low likelihood.

### L-3 (cosmetic) — `ALL CHECKS PASSED` appears twice

Once on the `-KeepInstalled` path and once on the normal path. Both are success
messages; harmless.

## Security notes

- The install-directory ACL grants `Users` read+execute only, and the installer
  fails if the applied ACL still lets `Users` write/modify — the DLL-hijack path is
  closed (SVC-03).
- The status file is written atomically and unit-tested to contain no configuration
  values, paths, PINs, or key material.
- No new `unsafe`, and the single-Send/Sync invariant test still passes.
- The restart action depends on a non-zero exit; the service now reports
  `ServiceSpecific(1..4)` on startup failure instead of a clean 0.
- The degraded provider path (`UnavailableProvider`) preserves the Linux CI startup
  and does not silently claim a working vendor library.

## Verification

- `cargo test` → 142 passed, 0 failed
- `cargo test --test contract_fixtures` → 37/37
- `cargo fmt --all -- --check` → 0
- `cargo clippy --all-targets -- -D warnings` → 0
- `cargo check --target i686-pc-windows-gnu` → 0
  (the optional `x86_64-pc-windows-gnu` target is not installed here)

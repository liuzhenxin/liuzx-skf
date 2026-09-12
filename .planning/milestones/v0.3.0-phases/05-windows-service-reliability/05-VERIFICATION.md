---
phase: 05
slug: windows-service-reliability
status: passed
verified_at: 2026-09-11
must_haves_total: 5
must_haves_verified: 5
must_haves_partial: 0
requirements_verified: [SVC-01, SVC-02, SVC-03, SVC-04, SVC-05]
requirements_partial: []
human_verification_count: 6
verifier: inline (no subagent runtime available)
---

# Phase 5 — Verification

**Phase goal:** 让 Windows 服务如实反映自身可用性，并让安装、升级、卸载在权限与清理上可靠。

**Verdict:** `passed` for the code and configuration that can be verified here.
The SCM runtime behaviour (live ordering, restart firing, ACL enforcement, file
locks) is inherently Windows-only and is recorded for human verification. All
platform-independent logic is unit-tested, the crate cross-compiles for i686
Windows, and the frozen contract is 37/37.

## Automated Evidence

| Check | Command | Result |
|-------|---------|--------|
| Full suite | `cargo test` | 142 passed, 0 failed |
| Startup logic | `cargo test server::` | 5 passed |
| Status/exit codes | `cargo test service_state` | 4 passed |
| Contract replay | `cargo test --test contract_fixtures` | 37/37 |
| Formatting | `cargo fmt --all -- --check` | exit 0 |
| Lint | `cargo clippy --all-targets -- -D warnings` | exit 0 |
| Windows cross-compile | `cargo check --target i686-pc-windows-gnu` | exit 0 |
| Schema drift | `gsd-tools verify schema-drift 05` | `drift_detected: false` |

## must_haves

| # | Must-have | Status | Evidence |
|---|-----------|--------|----------|
| 1 | `status` reports the startup stage; `Running` only after bind | **Met (code + unit)**, live ordering Windows UAT | `service_state.rs` + `run_service` diff; `bind_with_progress` order test |
| 2 | Invalid configuration exits with a distinct non-zero code and restart fires | **Met (code + unit)** | `StartupError` → `EXIT_CONFIG/PROVIDER/BIND/SERVE`; unit tests assert `1/2/3` |
| 3 | Install directory prevents a non-admin replacing exe/DLL | **Met (script)**; enforcement Windows UAT | `install.ps1` icacls `+` post-check; `verify-service.ps1` ACL assertion |
| 4 | Install/run/upgrade/uninstall leave no residue; reinstall works | **Met (script)**; runtime Windows UAT | `Wait-ServiceStoppedAndUnlocked`; strict removal; `verify-service.ps1` |
| 5 | `status` distinguishes process-running from usable | **Met (code + unit)** | `Startup:` line with `stage`/`code`/`address`; fallback to SCM output |

## ROADMAP Success Criteria

| # | Criterion | Status |
|---|-----------|--------|
| 1 | SCM sees StartPending during init, Running only after bind+provider | Met in code; live check in UAT |
| 2 | Invalid config exits non-zero and restart-on-failure fires | Met in code; live check in UAT |
| 3 | Install dir permissions block non-admin replacement | Met in script; enforcement in UAT |
| 4 | install/run/upgrade/uninstall leave no residue; reinstall succeeds | Met in script; runtime in UAT |
| 5 | `status` reports the failing/completed stage | Met in code; live check in UAT |

## Requirements

| Requirement | Status |
|-------------|--------|
| SVC-01 StartPending/Running ordering | Met (code + unit; live UAT) |
| SVC-02 distinct non-zero exit + restart | Met (code + unit; live UAT) |
| SVC-03 install-directory ACL | Met (script; enforcement UAT) |
| SVC-04 upgrade/uninstall without residue | Met (script; runtime UAT) |
| SVC-05 status stage reporting | Met (code + unit; live UAT) |

## Deviations

1. **D-01 `updated` is epoch seconds**, not ISO-8601 — avoids a date dependency.
2. **`x86_64-pc-windows-gnu` was not installed**, so the plan's optional second
   cross-check was skipped; the shipping `i686-pc-windows-gnu` target passed.
3. **PowerShell scripts were not executed** (macOS, no `pwsh`) — the Windows UAT
   script is provided and the runtime behaviour is a human item.

## Human Verification Required

`05-HUMAN-UAT.md` records the full Windows VM checklist: live StartPending→Running,
invalid-config restart, ACL enforcement, upgrade-over-running, reinstall, and
`status` distinguishing process vs usable.

## Notes

- The verifier ran inline because this runtime exposes no `Task` subagent API.

---
phase: 05-windows-service-reliability
plan: 01
subsystem: windows-service
tags: [svc-01, svc-02, svc-05, startup-reporting]

requires: []
provides:
  - src/service_state.rs (status file, atomic write, exit-code mapping)
  - server::bind_with_progress + StartupError
  - honest StartPending/Running reporting and stage-aware status
affects: [05-02]

tech-stack:
  added: []
  patterns:
    - "Startup phases are observable through a callback; failures carry a typed class that maps to a service exit code"

key-files:
  created:
    - src/service_state.rs
  modified:
    - src/lib.rs
    - src/server/mod.rs
    - src/win_service.rs

key-decisions:
  - "Configured-but-missing library degrades (UnavailableProvider); only a missing OS path is fatal — this preserves the Linux CI's vendor-free startup"
  - "The status file records epoch seconds, not an ISO string, to avoid a date dependency"
  - "The status file never carries configuration values or paths (unit-tested)"

requirements-completed: [SVC-01, SVC-02, SVC-05]

duration: 60min
completed: 2026-09-11
---

# Phase 5 Plan 1: Honest Startup Reporting and Distinct Exit Codes Summary

**The service now reports `StartPending` per phase, reports `Running` only after the listener is bound, exits with a distinct `ServiceSpecific` code on failure, and `status` names the phase.**

## What changed

| Piece | Behaviour |
|-------|-----------|
| `service_state.rs` | `StartupStage`, `ServiceStatusFile`, `write_atomic`/`read`, `EXIT_CONFIG=1/EXIT_PROVIDER=2/EXIT_BIND=3/EXIT_SERVE=4` |
| `server::bind_with_progress` | emits `config → provider → bind`; returns `StartupError{Config,Provider,Bind}` |
| `server::bind` | unchanged behaviour; delegates with a no-op callback |
| `run_service` | `StartPending(0)` → per-phase `StartPending(n)` → `Running` after bind → `serve`; failure writes `Failed` and reports `Stopped + ServiceSpecific(code)` |
| `status` | prints `Startup: <state> (stage=…, …)`, falling back to SCM-only output |

## Evidence

| Check | Result |
|-------|--------|
| `cargo test` | 142 passed, 0 failed |
| `cargo test server::` | 5 passed (order, provider error, config error, bind error, missing-lib degrades) |
| `cargo test service_state` | 4 passed (round-trip, atomic write, no-secret, distinct codes) |
| `cargo test --test contract_fixtures` | 37/37 |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --all-targets -- -D warnings` | exit 0 |
| `cargo check --target i686-pc-windows-gnu` | exit 0 |
| `Running` reported after bind | verified by reading the `run_service` diff (bind at line ~323, `report(Running)` at ~360) |

## D-08 / D-09 boundary

- Fatal: YAML unparseable → `StartupError::Config`; default alias has no path for
  this OS → `StartupError::Provider`.
- Non-fatal: the configured library file is missing or fails to load → the factory
  returns `UnavailableProvider`, the service still binds and reports `Running`.
  `configured_but_missing_library_still_binds` proves it, which is what keeps the
  Linux CI (no vendor library) working.

## Deviations

1. **`updated` is epoch seconds, not ISO-8601.** Formatting UTC without a date
   crate would be hand-rolled; the field is a string and only informational. Noted
   here rather than adding a dependency.
2. **`x86_64-pc-windows-gnu` was not installed**, so the optional second
   cross-check in the plan was skipped; `i686-pc-windows-gnu` (the shipping target)
   passed.

## Notes

- `service_state.rs` is platform-independent, so the SCM-facing logic that *uses*
  it is the only Windows-only part; the rest is covered by the normal test run.
- The windowed, real-SCM behaviour (ordering observed live, restart action firing,
  ACL, file locks) is a Windows human-UAT item.

## Self-Check: PASSED

- 142/0 tests, 37/37 contract, 0 warnings, i686 cross-check green
- startup failure maps to a distinct non-zero code; `Running` after `bind`

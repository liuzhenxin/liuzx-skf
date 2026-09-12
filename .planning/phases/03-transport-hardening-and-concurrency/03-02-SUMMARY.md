---
phase: 03-transport-hardening-and-concurrency
plan: 02
subsystem: provider
tags: [ffi-isolation, serialization, timeout, trans-03, trans-04, trans-05]

requires:
  - phase: 03-01
    provides: bounded transport
provides:
  - per-provider global FFI gate in native.rs (wait/cancel exempt)
  - whole-request blocking dispatch on the Tokio blocking pool
  - 30s request timeout for non-wait methods
affects: [03-03, 03-04, phase-6]

tech-stack:
  added: []
  patterns:
    - "One Arc<Mutex<()>> per provider, shared by all guards, taken in every FFI method and Drop"
    - "The dispatcher body runs inside spawn_blocking; async workers never touch the vendor library"
    - "Timeout wraps the JoinHandle: it stops the caller, not the vendor thread"

key-files:
  created:
    - tests/ffi_serialization.rs
  modified:
    - src/provider/native.rs
    - src/session/registry.rs
    - src/domain/*.rs
    - src/main.rs
    - tests/provider_invariants.rs
    - tests/session_support/mod.rs

key-decisions:
  - "The per-call spawn_blocking of CONTEXT D-07 was refined to whole-request blocking because guard Drop also performs blocking FFI; per-call alone would still block async workers on drop"
  - "The waiter uses the same lock as no other call: wait/cancel are the only concurrent pair and are exempt"
  - "Timeout is caller-side only; the vendor call cannot be aborted"
  - "SKF_FFI_TIMEOUT_SECONDS overrides the 30s bound for tests and operations"

requirements-completed: [TRANS-03, TRANS-04, TRANS-05]

duration: 75min
completed: 2026-09-11
---

# Phase 3 Plan 2: Blocking FFI Isolation, Serialization, and Timeout Summary

**Every vendor call — including guard `Drop` — now runs on the blocking pool behind one per-provider mutex, `WaitForDevEvent`/`CancelWaitForDevEvent` bypass the mutex, and non-wait requests time out at 30s; 37/37 fixtures still match.**

## Coverage

| Property | Evidence |
|----------|----------|
| Gate taken by every FFI method | 33 `gate_lock(&self.gate)` sites in `src/provider/native.rs` (29 methods + 4 `Drop`) |
| wait/cancel exempt | `wait_and_cancel_are_exempt_from_the_ffi_gate` (structural) |
| No new `unsafe impl` | `tests/provider_invariants.rs` still passes; invariant test counts exactly one `Send` and one `Sync` |
| Blocking dispatch | `tokio::task::spawn_blocking` is the only FFI path; domain handlers and `handle_request` are synchronous |
| Timeout | zero-timeout request returns `-1 "Device operation timed out after 30s"` deterministically (3/3 runs) |

## D-07 refinement (recorded deviation)

CONTEXT D-07 chose per-call `spawn_blocking`. The plan refined that to **whole-request blocking dispatch** because each guard's `Drop` calls the vendor library (`dis_connect_dev`, `close_application`, `close_container`, `close_hash`). Wrapping only explicit calls would leave those drops on async workers, so TRANS-04 would not hold. The semantics the user chose are preserved: blocking FFI runs on the blocking pool, the lock is per provider, and the timeout is caller-side. Only the isolation granularity changed (per request, not per call). Details:

- `SessionGuard` now stores `state: Option<SessionState>` with `take_state`/`put_state`, so the state travels to the blocking thread and back.
- The session language is moved with `std::mem::replace` and restored, preserving `SetLanguage` across requests.
- `WaitForDevEvent` no longer needs its own `spawn_blocking` (it is already on a blocking thread) and calls the provider directly.

## Evidence

- `cargo test` → **119 passed**, 0 failed.
  - lib 87, contract 4 (37/37), ffi_serialization 4, fixture_oracle 4, provider_invariants 7, session_invariants 6, session_isolation 2, session_lifecycle 2, transport_limits 3.
- `cargo check --all-targets` → 0 warnings.
- `cargo check --target i686-pc-windows-gnu` → success.

## Issues Encountered

- A zero-duration timeout raced a trivial `SetLanguage` request (the blocking task finished before the deadline was checked). The test now uses `IssueCertificate`, which shells out to OpenSSL and so is reliably pending; three consecutive runs passed.

## Self-Check: PASSED

- `grep -c 'gate_lock(&self.gate)' src/provider/native.rs` → 33
- `grep -c 'pub async fn handle' src/domain/*.rs` → 0 (all synchronous)
- `grep -c 'spawn_blocking' src/main.rs` → 1 (the dispatcher)
- `cargo test --test contract_fixtures` → 37/37

---
phase: 03
slug: transport-hardening-and-concurrency
status: passed
verified_at: 2026-09-11
must_haves_total: 6
must_haves_verified: 6
must_haves_partial: 0
requirements_verified: [TRANS-01, TRANS-02, TRANS-03, TRANS-04, TRANS-05, TRANS-06, TRANS-07]
requirements_partial: []
human_verification_count: 4
verifier: inline (no subagent runtime available)
---

# Phase 3 — Verification

**Phase goal:** 让服务在异常输入、缓慢设备与并发客户端下保持有界行为，并把厂商 DLL 的阻塞与线程安全风险限制在受控边界内。

**Verdict:** `passed`. All seven requirements have automated evidence; the frozen
contract is 37/37; the crate still has exactly one `unsafe impl Send` and one
`unsafe impl Sync`. Four items need real hardware or a Windows runner.

## Automated Evidence

| Check | Command | Result |
|-------|---------|--------|
| Full suite | `cargo test` | 133 passed, 0 failed |
| Contract replay | `cargo test --test contract_fixtures` | 37/37 |
| Transport limits | `cargo test --test transport_limits` | 3 passed |
| FFI isolation/serialization/timeout | `cargo test --test ffi_serialization` | 4 passed |
| Loopback gate | `cargo test --test loopback_gate` | 4 passed |
| Classification | `cargo test --test classification_invariants` | 4 passed |
| Provider invariants (1x Send/Sync) | `cargo test --test provider_invariants` | 7 passed |
| Payload boundary | `cargo test protocol` | 14 passed |
| Warnings | `cargo check --all-targets` | 0 |
| Windows-side compile | `cargo check --target i686-pc-windows-gnu` | success |
| Schema drift | `gsd-tools verify schema-drift 03` | `drift_detected: false` |

## must_haves

| # | Must-have | Status | Evidence |
|---|-----------|--------|----------|
| 1 | Frame/message cap 1 MiB; payload guard 256 KiB before decode; connection cap 64 | **Met** | `MAX_WS_MESSAGE_BYTES`, `Params::check_payload_len`, `Semaphore(64)`; transport tests |
| 2 | Every FFI call (including guard Drop) runs off async workers | **Met** | whole-dispatcher `spawn_blocking`; domain handlers synchronous; `ffi_serialization` behavioral test |
| 3 | One per-provider gate; wait/cancel exempt; no new unsafe | **Met** | 33 `gate_lock` sites; structural tests; `provider_invariants` |
| 4 | Non-wait requests times out at 30s; wait/cancel exempt | **Met** | `ffi_serialization::a_request_that_exceeds_30s_times_out`; `is_wait_method` |
| 5 | Non-loopback bind refused unless opted in | **Met** | `loopback_gate` 4 tests; `SkfConfig::allows_remote` |
| 6 | All 37 methods classified and documented | **Met** | `CLASSIFIED_METHODS`; `classification_invariants`; docs table |

## ROADMAP Success Criteria

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| 1 | Over-limit frame/payload rejected before large allocation; connections bounded | **Met** | tungstenite config + precheck + semaphore |
| 2 | Blocking calls off async workers; a stalled token does not stall unrelated clients | **Met, bounded** | Whole-request blocking; the documented boundary is: unrelated *non-token* requests and other providers are unaffected; same-provider requests queue on the gate (D-10) |
| 3 | A test fails if two operations use the same handle concurrently; one unsafe Send/Sync | **Met** | Every native method takes the gate (structural invariant fails if one is added without it); exactly one Send/Sync (invariant test). The gate makes concurrent same-handle use impossible rather than merely detected |
| 4 | Refuses non-loopback without opt-in | **Met** | `bind` pre-check on the resolved `SocketAddr` |
| 5 | Destructive operations distinguishable | **Met** | `OperationClass` + `docs/OPERATION-CLASSIFICATION.md` |

## Requirements

| Requirement | Status |
|-------------|--------|
| TRANS-01 frame/payload limits | Met |
| TRANS-02 connection bound | Met |
| TRANS-03 long operations bounded/cancellable | Met (timeout + wait/cancel pair) |
| TRANS-04 blocking calls off async workers | Met |
| TRANS-05 per-device serialization | Met (per-provider gate) |
| TRANS-06 non-loopback opt-in | Met |
| TRANS-07 destructive classification | Met |

## Deviations

1. **D-07 refinement:** per-call `spawn_blocking` became whole-request blocking
   because guard `Drop` also performs blocking FFI. Same semantics, coarser
   granularity.
2. **D-15 refinement:** the request-level timeout also bounds `IssueCertificate`'s
   OpenSSL subprocess, which D-15 had recorded as unbounded. This is a tighter
   bound and removes a known unbounded path; documented in `03-REVIEW.md` M-1.

## Human Verification Required

`03-HUMAN-UAT.md` records these; they need a real GM3000 or a Windows runner.

1. Two concurrent `SignData` calls on a real token do not crash the process and
   return correct results (the gate's real-world proof).
2. A genuinely slow device operation does not stop an unrelated client on another
   connection.
3. Whether the vendor DLL is safe for calls outside the gate (cannot be proven
   without the DLL's own concurrency bug).
4. Windows i686 artifact `Machine=0x014C` via the release workflow.

## Notes

- The verifier ran inline because this runtime exposes no `Task` subagent API; the
  commands a `gsd-verifier` agent would run were executed directly.

---
phase: 03
phase_name: transport-hardening-and-concurrency
status: clean
depth: standard
findings_total: 5
high: 0
medium: 1
low: 4
fixed: 0
reviewed_at: 2026-09-11
reviewer: inline (no subagent runtime available)
---

# Phase 3 Code Review

Scope: everything phase 3 changed — `src/server/mod.rs`, `src/protocol/params.rs`,
`src/config/mod.rs`, `src/provider/native.rs`, `src/session/registry.rs`,
`src/domain/*`, `src/main.rs`, and the four new integration test files.

The blocking handoff rules were applied: the scripted gate-lock insertion was
read method-by-method (each of the 33 sites is inside the intended function), and
bare greps were backed by comment-aware invariant tests.

## Findings

### M-1 (documented deviation, not a bug) — the request timeout also bounds `IssueCertificate`

CONTEXT D-15 scoped the timeout to provider FFI and recorded `IssueCertificate`'s
OpenSSL subprocess as an unbounded known limitation. The implementation applies
the timeout to the **whole request** (the natural shape once the dispatcher runs
as one blocking task), so `IssueCertificate` is now bounded at 30 s too. The
detached task continues and writes its temp files; the caller gets `-1`.

This is a **tighter** bound than the decision, not a looser one, and it removes
the unbounded path D-15 recorded. It is called out here, in `03-VERIFICATION.md`,
and in the summary so it is not a silent change. No fixture covers a slow
`IssueCertificate`, and the replay is green.

### L-1 (accepted) — a timed-out request leaves the session with an empty state

On timeout the connection is given `SessionState::new(ttl)` while the real state
finishes on the detached blocking thread. Consequences: the next request on that
connection is unauthenticated and holds no handles, and `guard.state().id()`
differs from the marker id until reconnect. This only occurs after an abnormal
timeout; resources are still released when the detached task ends. Documented for
the human verification pass.

### L-2 (accepted) — `check_payload_len` has a 1–2 byte slack

Base64 padding makes the encoded length of `N` and `N+1` decoded bytes equal, so a
length-only precheck cannot separate them exactly. The guard is an allocation
bound; the decoded ceiling is additionally bounded by the frame cap. Recorded in
`03-01-SUMMARY.md`.

### L-3 (accepted) — timeout resets the session language

`self.lang` is left at `EN` after a timeout because the real language moved into
the detached task. Only an abnormal path is affected; normal requests preserve
language (proven by the fixture replay, which uses `SetLanguage` in setup).

### L-4 (accepted) — detached blocking tasks are not joined

A timed-out task keeps one blocking-pool thread until the vendor returns. With the
64-connection cap and the per-provider gate, the number of such tasks is bounded by
client count, and each holds the gate only for its own provider. This is the
intended "stop the caller, not the vendor" semantics.

## Security notes

- Exactly one `unsafe impl Send` and one `unsafe impl Sync` remain
  (`tests/provider_invariants.rs` passes); the gate adds no unsafe.
- `WaitForDevEvent`/`CancelWaitForDevEvent` are exempt in both the gate
  (`wait_and_cancel_are_exempt_from_the_ffi_gate`) and the timeout
  (`is_wait_method`), so the cancel path cannot deadlock.
- A non-loopback WebSocket bind is refused before the socket opens; `0.0.0.0`/`::`
  are correctly non-loopback.
- The payload guard runs before `base64::decode`, so an oversized field cannot
  force a large allocation.
- No request path reads `OperationClass`; classification cannot change behaviour.

## Verification

- `cargo test` → 133 passed, 0 failed
- `cargo test --test contract_fixtures` → 37/37
- `cargo check --all-targets` → 0 warnings
- `cargo check --target i686-pc-windows-gnu` → success

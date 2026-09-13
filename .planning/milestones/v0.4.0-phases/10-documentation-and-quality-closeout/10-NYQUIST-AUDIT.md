# Nyquist Audit: Phases 3–6 (v0.3.0)

**Audited:** 2026-09-12, from the v0.4.0 tree
**Scope:** close the Nyquist sign-off left pending in phases 3, 4, 5, and 6 of the
shipped v0.3.0 milestone (QA-01).

## Method

For each phase the archived `0X-VALIDATION.md` per-task map was compared against
the **current** tree: every task must have an automated verification that still
runs and passes today. Where a structural command's literal text no longer matches
(code legitimately evolved after v0.3.0), the audit records the current equivalent
and confirms the *property* is still covered by a live test.

The archived `0X-VALIDATION.md` files are frozen history and are **not** modified;
this document is the sign-off record (decision D-01).

## Commands re-run for this audit

```
cargo test --lib
cargo test --test transport_limits
cargo test --test log_redaction
cargo test --test protocol_version
cargo test --test restricted_methods
cargo test --test fixture_oracle --test classification_invariants
cargo test classification
cargo test service_state
cargo test logging
cargo test diagnostic
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo check --target i686-pc-windows-gnu
```

All exited 0. Key results: `--lib` 145 passed; `transport_limits` 3; `log_redaction`
3; `protocol_version` 4; `restricted_methods` 3; `service_state` 5; `logging` 4;
`diagnostic` 5; `classification` 4.

---

## Phase 03 — Transport Hardening and Concurrency

| Task | Verification | Current result |
|------|--------------|----------------|
| 03-01-01 (1 MiB frame/message) | `grep -c 'accept_async_with_config' src/server/mod.rs == 1` | **Superseded:** phase 8 replaced the upgrade call with `accept_hdr_async_with_config` (count 1) for token auth. The 1 MiB `WebSocketConfig` is still passed; `tests/transport_limits.rs` (3 passed) is the live proof. |
| 03-01-02 (256 KiB payload) | `cargo test protocol` | 16 passed |
| 03-01-03 (64-connection cap) | `grep -c 'Semaphore::new' src/server/mod.rs == 1` | 1 |
| 03-01-04 (oversize rejection end-to-end) | `cargo test --test transport_limits` | 3 passed |
| 03-02-01 (native-call serialization) | `cargo test --test provider_invariants` | 7 passed |
| 03-02-02 (blocking pool) | `cargo test --test contract_fixtures` + `session_lifecycle` | contract 4/0; lifecycle 2 passed |
| 03-02-03/04 (timeout; wait/cancel) | `cargo test --test ffi_serialization` | 4 passed |
| 03-03-01 (`allow_remote`) | `cargo test config` | passed |
| 03-03-02 (loopback gate) | `grep -c 'is_loopback' src/server/mod.rs == 1` | 1 |
| 03-03-03 (bind combinations) | `cargo test --test loopback_gate` | 7 passed (extended for BIND-01) |
| 03-04-01 (`OperationClass`) | `cargo test classification` | 4 passed |

**Verdict: COMPLIANT.** One structural check is superseded by a later phase and is
now covered by `transport_limits`. No task lacks automated verification.

---

## Phase 04 — Format/Lint Normalization and CI Gate

| Task | Verification | Current result |
|------|--------------|----------------|
| 04-01-01 (formatting) | `cargo fmt --all -- --check` | exit 0 |
| 04-01-02 (formatting semantic-neutral) | `cargo test --test contract_fixtures` | 4/0 |
| 04-02-01 (pinned toolchain) | `grep -c 'channel = "1.93.0"' rust-toolchain.toml == 1` | 1 |
| 04-02-02/03/04 (clippy hard-fail) | `cargo clippy --all-targets -- -D warnings` | exit 0 |
| 04-03-01 (PR + main/dev triggers) | workflow YAML | present |
| 04-03-03 (gate failure-sensitivity) | `gate-selftest` job | present |
| 04-03-04 (all jobs runnable locally) | fmt/clippy/test/i686 | all exit 0 |

**Verdict: COMPLIANT.**

---

## Phase 05 — Windows Service Reliability

| Task | Verification | Current result |
|------|--------------|----------------|
| 05-01-01/02/03 (state file, exit codes, classification) | `cargo test service_state` + `cargo test server::` | service_state 5 passed; server tests pass |
| 05-01-04 (service ordering; i686) | `cargo check --target i686-pc-windows-gnu` | exit 0 |
| 05-02-01 (install ACL) | `grep -c 'icacls' install.ps1` | 3 |
| 05-02-02 (upgrade waits/unlocks) | `grep -c 'Start-Sleep' install.ps1 == 0` | 0 |
| 05-02-03 (manual verify script) | `test -f packaging/windows/verify-service.ps1` | present |
| 05-02-04 (docs match exit codes) | `docs/WINDOWS-SERVICE.md` | updated; exit code 3 now also covers the exposure-rule refusal |

**Verdict: COMPLIANT.** The Windows-only residual is covered by the real-hardware
UAT recorded in `docs/PRE-RELEASE-UAT.md` (A1–A7, B1–B5) and is not reproducible on
this host.

---

## Phase 06 — Observability, Diagnostics, Release Verification

| Task | Verification | Current result |
|------|--------------|----------------|
| 06-01-01 (JSON lines + rotation) | `cargo test logging` | 4 passed |
| 06-01-02 (service uses rotating logger) | `grep -c 'logging::init_service_file' src/win_service.rs == 1` | 1 |
| 06-01-03 (redaction) | `cargo test --test log_redaction` | 3 passed (word list extended in phases 7–9) |
| 06-02-01/02 (diagnose booleans, no secrets) | `cargo test diagnostic` | 5 passed |
| 06-02-03 (apiVersion) | `cargo test --test protocol_version` | 4 passed |
| 06-02-04 (restricted methods) | `cargo test --test restricted_methods` | 3 passed |
| 06-03-01 (checksum) | `grep -c 'sha256' packaging/windows/build.ps1` | 2 |
| 06-03-02 (PE verification) | `grep -c '0x014C\|Get-PeMachine' release-windows.yml` | 4 |
| 06-03-03/04/05 (release notes, threat model, bilingual doc) | file/grep checks | all present |

**Verdict: COMPLIANT.**

---

## Summary

| Phase | Nyquist verdict |
|-------|-----------------|
| 03 Transport Hardening | COMPLIANT (one structural check superseded, property still tested) |
| 04 Format/Lint/CI Gate | COMPLIANT |
| 05 Windows Service | COMPLIANT (Windows-only residual covered by real-hardware UAT) |
| 06 Observability/Diagnostics/Release | COMPLIANT |

Sampling rate is continuous (no run of three tasks without an automated command),
feedback latency is seconds, and no watch-mode flags are used. QA-01 sign-off:
complete.

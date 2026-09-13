# Phase 7 Verification: TLS Termination

**Verified:** 2026-09-12
**Host:** macOS (development); USB token attached to the Windows VM
**Method:** inline execution (no subagent tool in this runtime)

## Requirements

| Requirement | Status | Evidence |
|-------------|--------|----------|
| TLS-01 (configured cert/key enables TLS; load failure is a distinct startup failure) | PASS | `server::build_tls_acceptor`; `tests/tls.rs::tls_client_handshake_and_call`; `StartupError::Config` unit tests |
| TLS-02 (TLS client completes handshake and methods work) | PASS | `tests/tls.rs::tls_client_handshake_and_call` (`SetLanguage` → `error: 0`) |
| TLS-03 (disabled by default, plaintext otherwise) | PASS | `tests/tls.rs::plaintext_default_still_works`, `::tls_not_configured_means_plaintext` |
| TLS-04 (key/cert never in logs, diagnose, status file, release metadata) | PASS | `tests/tls.rs::tls_key_never_appears_in_diagnose_or_logs`; extended `tests/log_redaction.rs`; `src/service_state.rs` has no TLS field |
| COMPAT-01 (v0.2.0 loopback client unchanged; 37 fixtures match) | PARTIAL | Plaintext default proven by tests. The 37-fixture replay is **blocked on hardware** on this host (see below). |

## Commands run

```bash
cargo test --lib                       # 121 passed, 0 failed
cargo test --no-fail-fast              # all suites green except contract_fixtures
cargo check --all-targets              # clean
cargo check --target i686-pc-windows-gnu   # clean (ring provider)
cargo fmt --all -- --check             # clean
cargo clippy --all-targets -- -D warnings  # clean
```

## Hardware blocker (pre-existing, not introduced here)

`cargo test --test contract_fixtures` reports `contract replay: 13 compared, 2
shape-checked, 22 failed`. Every failure has the same shape:

```
expected: {"error":167772195,"id":1,"message":"ConnectDev failed: 0x0A000023"}
actual:   {"error":1,"id":1,"message":"ConnectDev failed: 0x00000001"}
```

`0x0A000023` is recorded from a run with the GM3000 token attached to this Mac;
`0x00000001` is what `ConnectDev` returns when the token is attached to the Windows
VM instead. The `IssueCertificate` fixture remains shape-checked (OpenSSL-version
volatile). No dispatcher code changed in this phase, so this is an environment
difference, not a regression. It must be re-run with the token on this Mac (or on
the Windows VM as a recorded baseline) before the milestone is audited.

## Nyquist / sampling

- Every task has an automated command (see `07-VALIDATION.md`).
- Quick loop: `cargo test --lib` (~0.1 s). Full loop: `cargo test` (~4 s).
- No watch-mode flags; `cargo check --target i686-pc-windows-gnu` is the slowest
  gate (~80 s cold) and is run per wave.

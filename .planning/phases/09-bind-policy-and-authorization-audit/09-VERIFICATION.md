# Phase 9 Verification: Bind Policy and Authorization Audit

**Verified:** 2026-09-12 (inline execution)
**Host:** macOS; GM3000 token now attached to this Mac

## Requirements

| Requirement | Status | Evidence |
|-------------|--------|----------|
| BIND-01 (non-loopback needs opt-in + TLS + client auth) | PASS | `bind_with_progress` combined rule; `remote_bind_without_tls_is_a_bind_error`, `remote_bind_with_tls_and_auth_is_allowed`, `non_loopback_without_tls_is_refused`, `non_loopback_without_client_auth_is_refused` |
| BIND-02 (refusal before bind, names requirements, distinct exit code) | PASS | check precedes `TcpListener::bind`; message names all three; `exit_code() == EXIT_BIND` asserted |
| BIND-03 (loopback unchanged; tests updated) | PASS | `loopback_is_allowed_without_opt_in`, `loopback_bind_needs_no_opt_in_tls_or_auth`, `loopback_without_client_auth_still_binds` |
| AUD-01 (grants/deny/expiry/device-unavailable audited) | PASS | `tests/audit.rs`; `SessionState` emits all four events |
| AUD-02 (no PIN/key/payload/token in audit) | PASS | event fields by construction; `audit_json_has_no_secret_fields`, `audit_records_never_carry_secrets`; `log_redaction` green |

## Commands run

```bash
cargo test --lib                 # 145 passed
cargo test --no-fail-fast        # all suites green
cargo test --test loopback_gate  # 7 passed
cargo test --test audit          # 2 passed
cargo check --target i686-pc-windows-gnu
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

## Blocker resolution

The phase-7/8 hardware blocker is **resolved**: the GM3000 token is attached to
this Mac again, so `cargo test --test contract_fixtures` now reports
**4 passed / 0 failed** (37 fixtures replayed). The v0.4.0 COMPAT-01 evidence is
now complete.

## Note

The audit proof is library-level rather than a subprocess stderr capture, because
an absent token makes every handler fail before it reaches the authorization check.
The capture drives the exact `SessionState` → `audit::emit` path the service uses.

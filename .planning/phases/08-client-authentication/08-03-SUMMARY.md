---
plan: 08-03
phase: 8
status: complete
completed: 2026-09-12
requirements: [AUTH-01, AUTH-02, AUTH-03, AUTH-04]
---

# Plan 08-03 Summary: Bind Coupling and Hermetic Client-Auth Tests

## What shipped

- `bind_with_progress` refuses a non-loopback bind when
  `ClientAuthMode::requires_loopback()` (i.e. `none`), before opening the socket
  (AUTH-03). Message names `tls.client_auth`; exit code is `EXIT_BIND` (3).
- `tests/loopback_gate.rs`: the two opt-in positives now carry `client_auth:
  token`; new negatives/positives cover "opt-in + none refused" and "loopback +
  none allowed".
- `tests/client_auth.rs` (hermetic, rcgen): mTLS no-cert refused, valid cert
  works, untrusted CA refused; token missing/wrong refused, correct accepted;
  token works without TLS on loopback; token never in diagnose or logs.
- `tests/log_redaction.rs` treats `token`/`bearer`/`client_cert` as sensitive.

## Evidence

| Check | Result |
|-------|--------|
| `cargo test --test loopback_gate` | 6 passed |
| `cargo test --test client_auth` | 6 passed |
| `cargo test --test log_redaction` | 3 passed |
| `cargo check --target i686-pc-windows-gnu` | pass |
| `cargo fmt --all -- --check` / `clippy -D warnings` | clean |
| 37 frozen fixtures | hardware-blocked on this host (token on the Windows VM) — see `08-VERIFICATION.md` |

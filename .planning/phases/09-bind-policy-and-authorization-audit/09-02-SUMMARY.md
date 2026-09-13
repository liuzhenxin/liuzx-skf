---
plan: 09-02
phase: 9
status: complete
completed: 2026-09-12
requirements: [BIND-01, BIND-02, BIND-03]
---

# Plan 09-02 Summary: Combined Exposure Rule

## What shipped

- `bind_with_progress` now has one rule: a non-loopback bind requires
  `allows_remote() && tls.is_some() && !client_auth.requires_loopback()`.
- Refusal happens before `TcpListener::bind`, names all three requirements, and
  returns `StartupError::Bind` (exit code `EXIT_BIND` = 3).
- Loopback unchanged: no opt-in, TLS, or auth required.

## Message

```
refusing to bind WebSocket listener to non-loopback address <addr>; remote
binding requires all of: allow_remote: true (or SKF_ALLOW_REMOTE=1), TLS
(tls.cert_file and tls.key_file), and client authentication (tls.client_auth:
mtls or token)
```

## Evidence

| Check | Result |
|-------|--------|
| `cargo test server::` | 145 lib tests pass, incl. `remote_bind_without_tls_is_a_bind_error`, `remote_bind_with_tls_and_auth_is_allowed`, `loopback_bind_needs_no_opt_in_tls_or_auth` |
| `cargo test --test loopback_gate` | 7 passed (3 negatives, 2 TLS positives, 2 loopback) |
| `cargo check --target i686-pc-windows-gnu` | pass |

## BIND-03

`tests/loopback_gate.rs` positives now generate TLS material (rcgen) and carry a
token; a new `non_loopback_without_tls_is_refused` proves the TLS half, and the
loopback tests remain configuration-free.

---
plan: 07-01
phase: 7
status: complete
completed: 2026-09-12
requirements: [TLS-01, TLS-02, TLS-03, COMPAT-01]
---

# Plan 07-01 Summary: Optional TLS Termination

## What shipped

Server-side TLS is now an **opt-in** capability of the WebSocket listener. With no
`tls:` block the service is byte-for-byte the v0.3.1 plaintext loopback listener.

| Area | Change |
|------|--------|
| Config | `SkfConfig.tls: Option<TlsConfig>` (named field, so the flattened `libs` map is untouched) |
| Env overrides | `SKF_TLS_CERT`, `SKF_TLS_KEY`, `SKF_TLS_CLIENT_AUTH` |
| Validation | Half-configured pair or `client_auth != none` is an error before any socket is bound |
| Loading | `server::build_tls_acceptor` reads PEM with `rustls-pemfile`, builds a `ServerConfig` with the explicit **`ring`** provider, `with_no_client_auth()` |
| Failure mode | Any load/build failure → `StartupError::Config` (exit code **1**). No plaintext fallback. |
| Connection path | `run_connection<S, T>` is generic over the stream; plaintext `TcpStream` and `TlsStream<TcpStream>` share one loop |
| Concurrency | The semaphore permit is acquired in `admit` and held across the TLS handshake; a failed handshake logs a class and drops the connection without stopping the accept loop |
| Diagnostics | `diagnose` reports `tls_enabled` / `tls_cert_loaded` booleans only |

## Versions

- `rustls 0.23.44` (default-features off; `std`, `ring`, `logging`, `tls12`)
- `tokio-rustls 0.26.5` (`ring`, `logging`, `tls12`)
- `rustls-pemfile 2.2.0`, `rustls-pki-types 1.15.1`
- dev: `rcgen 0.14.10` (`crypto`, `pem`, `ring`)

## Config example (disabled by default)

```yaml
default: GM3000
vendor: {}
tls:
  cert_file: config/server.crt
  key_file: config/server.key
  client_auth: none   # mtls/token arrive in phase 8
GM3000:
  windows: native\GM3000\windows\mtoken_gm3000.dll
```

## Evidence

| Check | Result |
|-------|--------|
| `cargo test --lib` | 121 passed, 0 failed |
| `cargo test --test tls` | 4 passed (plaintext default, TLS happy path, TLS refusal on plaintext, secret-safety) |
| `cargo check --target i686-pc-windows-gnu` | **pass** (ring cross-compiles) |
| `cargo fmt --all -- --check` / `cargo clippy --all-targets -- -D warnings` | pass |
| 37 frozen fixtures | **not re-verified on this host**: the USB token is attached to the Windows VM, so every `ConnectDev` fixture returns `0x00000001` instead of the recorded `0x0A000023`. The failure is orthogonal to this plan (the dispatcher's `ConnectDev` path is untouched). See `07-VERIFICATION.md`. |

## Deviations

- TLS construction landed in `src/server/mod.rs` (`build_tls_acceptor`) rather than a
  separate module, to keep the plan's grep-verifiable acceptance criteria meaningful.
  A crate-level `test_env` lock was added so env-driven unit tests cannot race.
- `rcgen` resolved to 0.14.10 (the plan guessed 0.13); the API used is identical.

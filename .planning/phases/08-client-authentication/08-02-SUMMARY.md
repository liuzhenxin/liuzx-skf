---
plan: 08-02
phase: 8
status: complete
completed: 2026-09-12
requirements: [AUTH-01, AUTH-02, AUTH-04]
---

# Plan 08-02 Summary: Server-Side mTLS and Bearer-Token Enforcement

## What shipped

- `build_tls_acceptor(config, mode)` installs
  `rustls::server::WebPkiClientVerifier::builder(Arc<RootCertStore>)` for `mtls`
  and `.with_no_client_auth()` otherwise. Missing/unusable CA is a startup error.
- `PreparedServer` carries `client_auth: ClientAuthMode` and
  `token: Option<Arc<[u8]>>`; the startup line is `tls=… client_auth=…`.
- All connections upgrade through `accept_hdr_async_with_config`. In `token`
  mode the callback compares `Authorization: Bearer <token>` with a constant-time
  check and returns `401 Unauthorized` before the session is created.
- Identity extraction: mTLS → `subject_cn_from_der` (x509-parser) →
  `Certificate { subject_cn }`; token → `Token`; else `Anonymous`.
- `SessionFactory::create(ClientIdentity)`; `JsonRpcSession` stores the identity
  and `main.rs` logs only `identity.class()`.

## Evidence

| Check | Result |
|-------|--------|
| `cargo check --all-targets` | clean, 0 warnings |
| `cargo test --lib` | 136 passed (incl. mtls verifier, no-CA error, token-without-TLS, CN extraction) |
| `cargo check --target i686-pc-windows-gnu` | pass |
| Token/identity never logged | covered by 08-03's capture test |

## Note

Client-certificate rejection happens inside the rustls handshake, so it reuses
the phase-7 "log a class, drop the connection, keep accepting" path. No new
error surface was added.

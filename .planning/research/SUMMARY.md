# v0.4.0 Research — Summary

**Milestone:** v0.4.0 Secure Remote Operation.

## Stack additions (verified in fetched sources)

- `rustls 0.23` with the **`ring`** provider, `tokio-rustls 0.26` (`ring`),
  `rustls-pemfile 2` for PEM loading. No `native-tls`/OpenSSL; no
  `tokio-tungstenite` TLS feature (server-side accept only).
- Test-only: `rcgen` (dev-dependency) for hermetic certificates, or a checked-in
  test cert/key pair.

## Feature table stakes

TLS on the WebSocket listener; client authentication (mTLS or bearer token);
non-loopback allowed only with remote-opt-in **and** TLS **and** client auth;
authorization-decision audit (non-sensitive); v0.2.0 loopback clients unchanged.

## Architecture

`TcpListener::accept -> admit(permit) -> TlsAcceptor.accept -> TlsStream ->
accept_hdr_async_with_config -> spawn_client`. Build the `Arc<ServerConfig>` once at
startup (in `bind_with_progress`, so cert/key errors are fatal `StartupError`s);
share it like the provider. Add a named `tls:` config block; env overrides for
operations/tests. Audit through `logging` with fixed, non-sensitive fields.

## Watch out for

- rustls defaults to `aws-lc-rs` → **pin `ring`** or the i686 Windows cross-build
  breaks.
- Handshake failure must not stop the accept loop; the semaphore permit must cover
  the handshake.
- Sensitive material (key, token, client CA) must never reach logs, `diagnose`, the
  status file, or release metadata.
- Keep TLS additive: no TLS by default, and the 37 fixtures must stay green.
- Hermetic test certs (`rcgen`) — do not assert on OpenSSL CLI text.

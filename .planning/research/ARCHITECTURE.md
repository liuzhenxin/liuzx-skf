# v0.4.0 Research — Architecture

## Where TLS attaches

```
TcpListener::accept -> admit (connection cap) -> TLSAcceptor.accept(stream)
                                                       |
                                          TlsStream<...> passed to
                                          accept_hdr_async_with_config(...)
                                                       |
                                             spawn_client request loop
```

- **`tokio_rustls::TlsAcceptor`** (built once from an `Arc<rustls::ServerConfig>`)
  wraps the TCP stream: `acceptor.accept(tcp).await -> TlsStream<TcpStream>`.
- The WebSocket upgrade stays `tokio_tungstenite::accept_hdr_async_with_config`
  (header callback added so a bearer token can be read from the upgrade request and
  the configured frame/message limits are preserved: 1 MiB).
- `TlsStream` implements `AsyncRead + AsyncWrite`, so the existing `spawn_client`
  body is unchanged once it receives the stream.

## Server construction

- `bind_with_progress` (phase 5) resolves config, provider, and binds. Add TLS
  material loading there and return `StartupError::Config` on a bad cert/key, so the
  Windows service still exits with a distinct code.
- Build one `Arc<ServerConfig>` at startup and share it with `serve`/`admit` (like
  the provider/session factory). Handshake happens per connection on the blocking
  path? No — `acceptor.accept()` is async and must run on the runtime; it is
  network I/O, not vendor FFI, so it stays on the async worker.

## Client authentication

- **mTLS:** `rustls::server::WebPkiClientVerifier::builder(Arc<RootCertStore>)` ->
  `.build()` -> set via `ServerConfig::builder().with_client_cert_verifier(...)`.
  The peer certificate is available on the `TlsStream` after the handshake; extract
  the subject for the audit record (do not log the whole certificate).
- **Bearer token:** read the `Authorization` header in the `accept_hdr_async`
  callback; compare against the configured secret in constant time; reject the
  upgrade (HTTP 401) on mismatch. Do not accept the connection.

## Bind policy

`src/server/mod.rs::bind_with_progress` currently refuses non-loopback unless
`allows_remote()`. New rule: non-loopback is allowed only when
`allows_remote() && tls_enabled && client_auth_enabled`. Refusal stays
`StartupError::Bind` (exit 3) with an explicit message naming both requirements.

## Audit events

- Emit `log::info!`/`warn!` records at authorization decisions:
  `event=auth decision=allow|deny actor=mtls:<cn>|token|anonymous provider=<> device=<> reason=<class>`.
- Route through `logging` (JSON lines) and stay within the redaction scan
  (no `pin`/`key`/`decrypted`/`payload` tokens).
- On the bearer path, never log the token; on mTLS, log the subject CN only.

## Configuration

- Add a named `tls: Option<TlsConfig>` to `SkfConfig` (named field binds before the
  flattened provider map; do not add an unnamed nested key).
- Private key and bearer token come from files/env; `diagnose` must report only
  "tls enabled/disabled, client-auth mode, cert loaded yes/no" — never paths or
  secrets (extend `diagnostic.rs`).

## Testing seams

- **Unit/integration TLS tests without hardware:** generate a self-signed cert and a
  client CA at test time (rustls `ring` can sign test certs via a small helper, or
  shell out to `openssl` where available — prefer an in-repo test helper using
  `rcgen`? `rcgen` is not a dependency; see PITFALLS for the decision).
- `tests/transport_limits.rs` and the contract replay must keep using plaintext
  loopback by default, so TLS is additive and does not perturb the 37 fixtures.

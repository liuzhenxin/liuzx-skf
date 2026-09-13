# Phase 7: TLS Termination — Research

**Researched:** 2026-09-12
**Method:** project-level research already fetched the crates; this phase document
narrows it to the concrete APIs and integration points in *this* repository.

## Verified crate APIs (from the extracted sources)

- `rustls 0.23`: `ServerConfig::builder_with_provider(Arc<CryptoProvider>)` +
  `.with_safe_default_protocol_versions()?` + `.with_no_client_auth()` +
  `.with_single_cert(Vec<CertificateDer>, PrivateKeyDer)`. `rustls::crypto::ring::default_provider()`
  gives the `ring` provider explicitly (avoids the aws-lc-rs default).
- `tokio-rustls 0.26.5`: `pub struct TlsAcceptor` (`src/server.rs:19`),
  `TlsAcceptor::from(Arc<ServerConfig>)`, `acceptor.accept(tcp).await -> io::Result<TlsStream<TcpStream>>`.
- `rustls-pemfile 2.2.0`: `certs(&mut dyn BufRead) -> impl Iterator<Item = Result<CertificateDer, io::Error>>`
  and `private_key(&mut dyn BufRead) -> io::Result<Option<PrivateKeyDer<'static>>>`.
- `tokio-tungstenite 0.21`: `accept_async_with_config` and `accept_hdr_async_with_config` accept any
  `S: AsyncRead + AsyncWrite + Unpin`; `TlsStream<TcpStream>` satisfies that. No TLS feature needed.

## Integration points in this repository

**`src/config/mod.rs`** — add a named field to `SkfConfig` (the `libs` map is
`#[serde(flatten)]`, so an unnamed top-level key breaks parsing; phase 2 D-07):

```rust
#[serde(default)]
pub tls: Option<TlsConfig>,

#[derive(Debug, Deserialize, Clone)]
pub struct TlsConfig {
    pub cert_file: Option<String>,
    pub key_file: Option<String>,
    #[serde(default)]
    pub client_auth: Option<String>, // "none" | "mtls" | "token"
}
```

Env overrides `SKF_TLS_CERT` / `SKF_TLS_KEY` / `SKF_TLS_CLIENT_AUTH`. A `tls` block
without both files is a configuration error. `client_auth` other than `none` is a
configuration error in this phase (phase 8 implements mtls/token).

**`src/server/mod.rs`** — build the acceptor in `bind_with_progress` so a bad
cert/key is a fatal `StartupError::Config` (exit 1):
- Load the PEM chain and key with `rustls_pemfile`, resolve the key type, build
  `ServerConfig` with the `ring` provider and `with_no_client_auth()`.
- `PreparedServer` gains `tls: Option<tokio_rustls::TlsAcceptor>` (the type is
  `Clone`; share by cloning into the accept task).
- `admit` acquires the semaphore permit, then spawns a task that performs the TLS
  handshake (when configured) before the WebSocket upgrade; a handshake error is
  logged as a class and the permit released. The accept loop never stops.
- Generalise the per-connection runner to `S: AsyncRead + AsyncWrite + Unpin + Send + 'static`
  so plaintext `TcpStream` and `TlsStream<TcpStream>` share one code path; keep the
  1 MiB `WebSocketConfig` and the session loop unchanged.

**`src/diagnostic.rs`** — add `tls_enabled: bool` and `tls_cert_loaded: bool` to
`DiagnosticReport`; never emit the path or key bytes. `render_text`/`render_json`
gain the fields. A unit test asserts the JSON does not contain the key file's path
or contents.

**`src/service_state.rs`** — unchanged: the status file stays free of TLS material.

**`src/logging.rs`** — a startup line may say `tls=disabled|enabled` only; no paths.

## Test strategy (hermetic)

- Add `rcgen` as a **dev-dependency**. At test time generate a self-signed server
  certificate for `localhost` (SAN `DNS:localhost`) and a client CA (for phase 8).
- `tests/tls.rs`:
  - Keeps the plaintext default: with no `tls:` block, a plaintext client works.
  - With a generated cert/key written to a temp dir + a temp YAML:
    - a `tokio-rustls` client trusting that cert completes the handshake and the
      WebSocket upgrade, and `SetLanguage` returns success;
    - `diagnose --config <temp>` reports `tls_enabled=true`,
      `tls_cert_loaded=true`, and its JSON contains neither the key path nor the key
      bytes;
    - a missing/unreadable cert/key makes the binary exit non-zero with an error
      that names only a class.
- Never assert on OpenSSL CLI output (the `IssueCertificate` lesson).

## Pitfalls carried into the plan

See `.planning/research/PITFALLS.md`. The phase-relevant ones: pin `ring`
(i686 cross-build); handshake failure must not stop the accept loop; the semaphore
permit must cover the handshake; the connection runner must be generic over the
stream; secrets never leave memory/files; `tls` must be a named config field.

## Validation Architecture

- **Quick:** `cargo test --lib tls` (config parsing, error classification).
- **Full:** `cargo test` + `cargo check --all-targets` + `cargo check --target i686-pc-windows-gnu`.
- **Seams:** `SkfConfig` parse; `bind_with_progress` TLS build + failure; per-connection
  TLS acceptance; `diagnose` non-sensitive booleans.
- **Manual/hardware:** none required for this phase (hermetic certs); the Windows
  service TLS variant is covered by the same commands.

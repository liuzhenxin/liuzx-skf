# v0.4.0 Research — Stack

**Scope:** only what the NEW capabilities (TLS termination, client authentication,
tightened bind policy, authorization audit) need. Everything validated in v0.3.0
(the provider seam, session/handles, transport bounds, logging, service lifecycle)
is not re-researched.

## Verified (from fetched crate sources, not memory)

| Crate | Version | Why | Evidence |
|-------|---------|-----|----------|
| `rustls` | `0.23.44` (constraint `0.23`) | TLS server, no OpenSSL/system dependency | `rustls-0.23.44/src/server/builder.rs` |
| `tokio-rustls` | `0.26.5` (`0.26`) | async `TlsAcceptor` over `TcpStream` | `tokio-rustls-0.26.5/src/server.rs:19` |
| `rustls-pemfile` | `2.2.0` (`2`) | load cert chain + private key from PEM | `rustls-pemfile-2.2.0/src/lib.rs:85,100` |
| `rustls-pki-types` | `1.15.1` (`1`) | `CertificateDer`/`PrivateKeyDer`, `ServerName`-adjacent types | pulled by rustls/pemfile |

**Crypto provider:** use the `ring` provider explicitly
(`rustls = { features = ["ring"] }`, `tokio-rustls = { features = ["ring"] }`) rather
than the `aws-lc-rs` default, because `ring` is already vendored in the dependency
graph and builds cleanly for `i686-pc-windows-gnu` (aws-lc-rs needs a C/cmake
toolchain that is fragile on the i686 cross target).

**No `tokio-tungstenite` TLS feature is required.** Its `rustls-tls-*` features are
for the *client* `connect` side. On the server we accept the TLS stream ourselves
and hand the `TlsStream` to the existing `accept_async_with_config` /
`accept_hdr_async_with_config`, so the dependency stays at `tokio-tungstenite 0.21`
with its default features.

## What NOT to add

- `native-tls` / `openssl` — pulls a system TLS stack; conflicts with the
  dependency-free, cross-compiled Windows story.
- `aws-lc-rs` — C toolchain fragility on `i686-pc-windows-gnu`.
- A new HTTP framework for auth — the bearer-token path can read the WebSocket
  upgrade request headers via `accept_hdr_async_with_config`.
- `jsonwebtoken`/`jose` — not needed: a static bearer token or mTLS client cert is
  sufficient for this milestone; JWT is a later concern.

## Config surface (respects the `#[serde(flatten)]` constraint)

`SkfConfig` uses a flattened provider map, so new configuration must be a **named**
field (a named `tls:` key binds before the flatten map) or an environment variable.

```yaml
tls:
  cert_file: "config/server.crt"      # PEM chain
  key_file: "config/server.key"       # PEM private key (sensitive)
  client_auth: mtls | token | none
  client_ca_file: "config/clients-ca.crt"   # required for mtls
```

Environment overrides for operations/tests: `SKF_TLS_CERT`, `SKF_TLS_KEY`,
`SKF_TLS_CLIENT_AUTH`, `SKF_TLS_CLIENT_CA`, `SKF_AUTH_TOKEN`. Secrets (private key
material, bearer token) are read from files/env and **never** logged.

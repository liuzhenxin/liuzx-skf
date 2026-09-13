# Phase 8: Client Authentication — Research

**Researched:** 2026-09-12
**Note:** no `08-CONTEXT.md` exists (discuss-phase was skipped). The decisions below
are grounded in the codebase and the already-fetched crate sources; they are
recorded here so the choices are visible and reviewable.

## Verified APIs (from the extracted sources)

- `rustls 0.23`: `rustls::server::WebPkiClientVerifier::builder(Arc<RootCertStore>)`
  → `ClientCertVerifierBuilder` (`.build()`), then
  `ServerConfig::builder…with_client_cert_verifier(verifier).with_single_cert(certs, key)`.
  `with_no_client_auth()` is the existing path (phase 7).
- `tokio-tungstenite 0.21` default features include `handshake`, so
  `tokio_tungstenite::accept_hdr_async_with_config(stream, callback, config)` is
  available. `Callback` = `FnOnce(&Request, Response) -> Result<Response, ErrorResponse>`;
  `ErrorResponse = http::Response<Option<String>>`, constructible with
  `http::Response::builder().status(StatusCode::UNAUTHORIZED).body(None)`.
  This is what lets the bearer token be checked **before** the WebSocket upgrade.
- `tokio-rustls`: after `acceptor.accept(stream).await`, the peer chain is
  `tls_stream.get_ref().1.peer_certificates()`.
- `x509-parser 0.15` (already a dependency): `parse_x509_certificate(der)` →
  `cert.subject().iter_common_name()`.
- `rcgen 0.14` (dev): `CertificateParams::new(sans)`, `params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained)`,
  `CertifiedIssuer::self_signed(params, key)`, `params.signed_by(&key, &issuer)`,
  `KeyPair::serialize_der()` (PKCS#8) — enough for a hermetic client CA + client cert.

## Decisions (defaults chosen, no discussion)

### D-01 — Modes and config shape
`tls.client_auth: none | mtls | token` (the field already exists). New optional
fields on `TlsConfig`:
- `client_ca_file` — PEM CA bundle used to verify client certificates (mTLS).
- `token_file` — file containing the bearer token (token mode).

Environment overrides: `SKF_TLS_CLIENT_CA`, `SKF_TLS_TOKEN` (the secret value
itself), `SKF_TLS_TOKEN_FILE`.

Rationale: the token is a secret, so it must not be written inline into the
committed YAML. Env or a file keeps it out of the config.

### D-02 — Validation matrix (fail-fast, no downgrade)
- `none` (default): no extra requirement.
- `mtls`: requires the server certificate **and** key (TLS must exist) **and**
  `client_ca_file`; otherwise a configuration error.
- `token`: requires a token source (`SKF_TLS_TOKEN` non-empty or a readable
  `token_file`); otherwise a configuration error. TLS is not required (the bind
  policy in phase 9 will require TLS for non-loopback).
- Any other value is a configuration error.

### D-03 — AUTH-03 predicate in phase 8, enforcement completed in phase 9
`ClientAuthMode::requires_loopback()` is `true` for `none`. Phase 8 wires the
"non-loopback ⇒ not `none`" rule into the existing bind gate and updates
`tests/loopback_gate.rs`. Phase 9 (`BIND-01`) adds the further requirement that
TLS also be enabled and consolidates the message/exit-code contract. The overlap
is deliberate: AUTH-03 is scoped to phase 8 and BIND-01 to phase 9.

### D-04 — Identity plumbing
`SessionFactory::create` gains a `ClientIdentity` argument:
`Anonymous` / `Token` / `Certificate { subject_cn }`. This is the smallest seam
change that satisfies AUTH-04 ("available to the session for audit"). Only
`src/main.rs` implements the trait, so the blast radius is two call sites.
The identity class is logged; the token value and certificate bytes are not.

### D-05 — Constant-time token comparison
Compare the `Authorization: Bearer <token>` value against the expected token with
a length-checked XOR accumulate, not `==`, so a timing side channel cannot leak
the token prefix. The length itself is not considered secret.

### D-06 — mTLS handshake failure is a connection failure, not a service failure
A client that cannot satisfy `WebPkiClientVerifier` fails inside
`acceptor.accept(stream)`, which the phase-7 code already turns into a logged
class + dropped connection. No change to the accept loop.

## Pitfalls carried into the plan

- Never log the token or the client certificate; extend the redaction scan with
  `token`/`bearer`/`client_cert`.
- The token callback runs inside the upgrade, so it must be `Send + 'static`;
  pass an `Arc<ClientAuth>` clone.
- `mtls` without TLS must be a startup error, never a plaintext listener with an
  ineffective client certificate check.
- `accept_hdr_async_with_config` replaces `accept_async_with_config` on **all**
  paths (including `none`) so there is a single connection code path.
- The phase-7 `plaintext_default_still_works` and 37-fixture expectations must
  not change: `none` remains the default and loopback is unaffected.
- The client-auth end-to-end tests need a client certificate signed by a CA the
  server trusts — generate the CA and the leaf in-process with rcgen.

## Validation Architecture

- **Quick:** `cargo test --lib` (config matrix, mode parsing, constant-time compare).
- **Full:** `cargo test` + `cargo check --target i686-pc-windows-gnu`.
- **Seams:** config validation; `build_tls_acceptor` verifier choice; the upgrade
  callback's token decision; identity extraction; the bind gate.
- **Manual/hardware:** none required (all hermetic). Real PKI client certificates
  are a deployment concern recorded for phase 10.

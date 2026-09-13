# Threat Model

This documents the trust boundary, the deliberate exposure and credential
decisions, and the known limitations of the v0.4.0 SKF gateway. It is written for
operators and reviewers; it is not a formal security proof.

## Assets

- **PIN** — the user credential for the token.
- **Private keys** — non-exportable key material on the token.
- **Session keys** — SM4 symmetric keys supplied per operation.
- **Plaintext payloads** — data to sign/encrypt/decrypt, and decrypted results.
- **Device control** — the ability to lock, format, or transmit arbitrary commands
  to the token.

## Trust boundary

| Boundary | Trust |
|----------|-------|
| WebSocket listener | Loopback by default (plaintext). A non-loopback bind requires **all** of explicit opt-in (`allow_remote` / `SKF_ALLOW_REMOTE`), TLS (`tls.cert_file` + `tls.key_file`), and client authentication (`tls.client_auth: mtls` or `token`). |
| HTTP demo server | Serves static files. In console mode it defaults to `0.0.0.0:8000`; this is a known boundary, not covered by the loopback gate. |
| Vendor library (GM3000 DLL) | Trusted, but its thread safety is undocumented and it is a `dlopen`/`LoadLibrary` target inside the install directory. |
| Service account | `LocalSystem`, required for GM3000 driver access. |
| Install directory | `%ProgramFiles(x86)%\LiuZX\SKF Service`, restricted by the installer to `SYSTEM`/`Administrators` full and `Users` read+execute. |
| Log file | Non-sensitive by policy; a source-scan test enforces it. Audit events are structured and non-sensitive by construction. |

## Transport security and client authentication (v0.4.0)

TLS is **opt-in**. With no `tls:` block the listener is plaintext loopback,
exactly as in v0.3.x, so existing clients are unaffected.

| Setting | Meaning |
|---------|---------|
| `tls.cert_file` / `tls.key_file` | PEM server certificate chain and private key. Both required to enable TLS. A load failure is a fatal configuration error (exit code `1`); the service never falls back to plaintext. |
| `tls.client_auth` | `none` (default), `mtls`, or `token`. An unknown value is a fatal configuration error. |
| `tls.client_ca_file` | PEM CA bundle used to verify client certificates. Required for `mtls`. |
| `tls.token_file` | File containing the bearer token, for `token` mode. |
| `SKF_TLS_CERT` / `SKF_TLS_KEY` / `SKF_TLS_CLIENT_CA` / `SKF_TLS_CLIENT_AUTH` | Environment overrides for the paths and mode. |
| `SKF_TLS_TOKEN` / `SKF_TLS_TOKEN_FILE` | The bearer-token value, or a file containing it. |

The TLS provider is `rustls` with the `ring` backend (chosen so the i686 Windows
build stays reproducible) and a minimum of TLS 1.2. The private key and the bearer
token are never written to logs, `diagnose` output, the status file, or release
metadata; `diagnose` reports only `tls_enabled` / `tls_cert_loaded` booleans.

The bearer token is a **secret and must not be written into the committed YAML**.
Supply it through `SKF_TLS_TOKEN` or a `token_file` whose permissions restrict
reading to the service account. `mtls` uses a client certificate verified against
`client_ca_file`; the verified subject CN is retained for audit, the certificate
bytes are not.

## Exposure decision

Remote exposure is allowed only when **all three** controls are configured:
explicit opt-in, TLS, and client authentication. The service refuses any other
non-loopback bind before opening the socket (exit code `3`) and names the missing
controls in the message. Loopback needs none of them.

`client_auth: none` is therefore acceptable only on loopback. A token may be used
without TLS on loopback; a non-loopback bind always requires TLS.

## Credential-retention decision

A verified PIN is **never retained**. `CheckPIN` records only a deadline
(`Grant { expires_at }`); a `String` cannot be wiped reliably, so the only
dependable guarantee is not to keep it. Authorization is therefore a
session-scoped, time-bounded grant, cleared on disconnect, on timeout, and when an
operation finds the device unavailable.

## Controls

- **Session isolation** — one session per connection; authorization and handles are
  fields of `SessionState`, reached through `&mut self`. A second session cannot
  name another session's handles.
- **Opaque handles** — `dev-N`/`app-N`/`cnt-N`/`hsh-N`, checked by prefix on every
  lookup; unknown, wrong-kind, and expired handles return `-11`.
- **Deterministic release** — resources are provider guards; `Drop` releases them,
  including on native error paths and on disconnect.
- **Serialization** — one global lock per provider library serializes native calls;
  `WaitForDevEvent`/`CancelWaitForDevEvent` are exempt so cancellation can run
  concurrently.
- **Bounded input** — 1 MiB frames, 256 KiB payloads (checked before decoding), 64
  connections, 30 s non-wait timeout.
- **Restricted methods** — `IssueCertificate` and `Transmit` can be disabled with
  `SKF_RESTRICT_LEGACY=1`; they never disappear.
- **Log redaction** — no request parameters, PINs, keys, or decrypted payloads are
  logged; a source scan fails the build if a log macro names one.
- **Authorization audit** — every authorization decision (`authorization.granted`,
  `authorization.denied` with a reason, `authorization.expired`,
  `device.unavailable` with a cleared count) is recorded as one structured JSON
  line. The event type can only carry provider, device, and application names, a
  reason, and a count, so no PIN, key, payload, or token can be recorded.
- **Install hardening** — restrictive directory ACL verified by the installer.
- **Release integrity** — SHA-256 checksum and PE32/i386 verification in the
  release workflow.

## Known limitations

- **TLS and client authentication are opt-in.** The default remains plaintext
  loopback, so a deployment that enables remote access must supply certificates and
  a token or client CA itself. Certificate rotation is manual.
- **A bearer token over plaintext is acceptable only on loopback.** Non-loopback
  binds always require TLS, so the token is never the sole control on a network.
- **There is no per-client authorization policy.** All authenticated clients have
  the same access to the configured provider/device operations; the identity (CN or
  token) is recorded for audit, not used to scope operations.
- **The HTTP demo can bind non-loopback** in console mode. It serves static files
  only; it is out of scope for the loopback gate.
- **`IssueCertificate` is a mock** and its OpenSSL subprocess is not bounded by the
  30 s request timeout (the timeout covers the whole request in the current
  implementation; see `RELEASE-NOTES.md`). Production certificates must come from a
  compliant external CA.
- **Vendor DLL thread safety is unproven.** Native calls are serialized per provider
  as a defensive default; relaxing to per-device locking would require vendor
  evidence.
- **A timeout stops the caller, not the vendor call.** The blocking thread keeps
  running until the library returns; the per-provider lock is held until then.
- **`WaitForDevEvent` blocks** and is intentionally excluded from the timeout.

## References

- `.planning/phases/02-session-authorization-and-resource-ownership/02-CONTEXT.md`
- `.planning/phases/03-transport-hardening-and-concurrency/03-CONTEXT.md`
- `.planning/phases/05-windows-service-reliability/05-CONTEXT.md`
- `.planning/phases/07-tls-termination/07-CONTEXT.md`
- `.planning/phases/08-client-authentication/08-RESEARCH.md`
- `.planning/phases/09-bind-policy-and-authorization-audit/09-RESEARCH.md`
- `docs/SESSION-AND-LIMITS.md`, `docs/OPERATION-CLASSIFICATION.md`,
  `docs/WINDOWS-SERVICE.md`

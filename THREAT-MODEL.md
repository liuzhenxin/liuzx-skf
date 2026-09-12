# Threat Model

This documents the trust boundary, the deliberate exposure and credential
decisions, and the known limitations of the v0.3.0 SKF gateway. It is written for
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
| WebSocket listener | Loopback by default. A non-loopback bind requires explicit opt-in (`allow_remote` / `SKF_ALLOW_REMOTE`). No TLS, no client authentication. |
| HTTP demo server | Serves static files. In console mode it defaults to `0.0.0.0:8000`; this is a known boundary, not covered by the loopback gate. |
| Vendor library (GM3000 DLL) | Trusted, but its thread safety is undocumented and it is a `dlopen`/`LoadLibrary` target inside the install directory. |
| Service account | `LocalSystem`, required for GM3000 driver access. |
| Install directory | `%ProgramFiles(x86)%\LiuZX\SKF Service`, restricted by the installer to `SYSTEM`/`Administrators` full and `Users` read+execute. |
| Log file | Non-sensitive by policy; a source-scan test enforces it. |

## Exposure decision

The protocol has no transport authentication or encryption, so v0.3.0 keeps the
service **loopback-only by default** and does not claim public-service capability.
Exposing it to a network requires an explicit opt-in and is the operator's
responsibility to place behind a trusted boundary. A future milestone may add TLS
and client authentication.

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
- **Install hardening** — restrictive directory ACL verified by the installer.
- **Release integrity** — SHA-256 checksum and PE32/i386 verification in the
  release workflow.

## Known limitations

- **No TLS or client authentication.** Network exposure depends on external
  controls; that is why loopback is the default.
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
- `docs/SESSION-AND-LIMITS.md`, `docs/OPERATION-CLASSIFICATION.md`,
  `docs/WINDOWS-SERVICE.md`

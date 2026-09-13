# v0.4.0 Research — Features

## Table stakes (the milestone is incomplete without these)

- **TLS on the WebSocket listener**: the server presents a certificate; a client can
  complete a TLS handshake before the WebSocket upgrade.
- **Client authentication**: unauthenticated clients cannot invoke methods. Two
  supported modes: **mTLS** (client certificate verified against a configured CA) or
  **bearer token** (a configured secret presented during the upgrade).
- **Tightened bind policy**: a non-loopback address is refused unless TLS is active
  **and** client authentication is enabled. `allow_remote` alone is no longer enough.
- **Non-loopback opt-in still required**: TLS does not silently enable remote
  binding; the operator must also opt in explicitly.
- **Authorization-decision audit**: allow/deny events are logged with non-sensitive
  facts (actor class, provider/device, result, reason class) — never PINs, keys, or
  payloads.
- **Backward compatibility**: a v0.2.0 client on loopback with no TLS keeps working
  (the default remains plain loopback), and the 37 fixtures stay green.

## Differentiators (valuable, can be phased)

- Per-client-certificate identity exposed to the session (subject CN) for audit and
  future per-client authorization.
- Configurable minimum TLS version and cipher defaults (rustls `ring` provider
  defaults are safe; document them).
- Handshake failure logging with a stable, non-sensitive error class (no peer
  certificate contents).

## Anti-features (do NOT build)

- A web PKI / ACME client, certificate issuance, or rotation automation.
- JWT/claim-based authorization, roles, or an external IdP.
- TLS for the static HTTP demo in this milestone (it is a local demo; document the
  boundary).
- Client-certificate revocation (CRL/OCSP) — defer.
- Storing the private key or bearer token in logs, diagnostics, or release metadata.

## Complexity notes

- mTLS is the larger of the two auth modes: it needs a client CA store, a rustls
  `ClientCertVerifier`, and per-connection extraction of the peer identity.
- Bearer token is small but must be compared in constant time and never logged.
- The bind-policy change touches the existing `allow_remote` gate and its tests.
- The audit events reuse `src/logging.rs`; they must pass the `log_redaction` scan.

## Dependencies on existing capabilities

- `server::bind_with_progress` / `serve` / `admit` / `spawn_client` (phases 3/5).
- `SessionState` for per-connection identity and grants (phase 2).
- `logging` (phase 6) and `service_state` (phase 5) for diagnostics.
- `config` named-field pattern and the `SkfConfig` flatten constraint.

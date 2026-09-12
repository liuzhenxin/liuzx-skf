# Requirements — Milestone v0.4.0 Secure Remote Operation

**Goal:** 让服务在非回环网络上有真实的传输安全与访问控制，移除"回环才是安全边界"这一根本限制。

**Prior milestone:** v0.3.0 / v0.3.1 Production Hardening (see `.planning/milestones/v0.3.0-REQUIREMENTS.md`).

---

## v0.4.0 Requirements

### TLS
- [ ] **TLS-01**: The WebSocket listener can run with TLS using a configured server certificate and private key; if either fails to load, startup fails with a distinct `StartupError`/exit code.
- [ ] **TLS-02**: A client that completes the TLS handshake can invoke the existing methods normally.
- [ ] **TLS-03**: TLS is disabled by default; with no TLS configuration the listener is plaintext and behavior is unchanged.
- [ ] **TLS-04**: Private-key and certificate material never appear in logs, `diagnose` output, the service status file, or release metadata.

### Client Authentication
- [ ] **AUTH-01**: With `client_auth: mtls`, a client certificate is required and verified against the configured client CA; a missing or invalid certificate is refused before any method runs.
- [ ] **AUTH-02**: With `client_auth: token`, the configured bearer token is required in the WebSocket upgrade request; a missing or incorrect token is refused.
- [ ] **AUTH-03**: `client_auth: none` is accepted only for a loopback bind (enforced together with the bind policy).
- [ ] **AUTH-04**: The authenticated identity (mTLS subject CN, or `token`) is available to the session for audit, and the token/private key are never logged.

### Bind Policy
- [ ] **BIND-01**: A non-loopback bind is allowed only when remote opt-in is set AND TLS is enabled AND client authentication is enabled.
- [ ] **BIND-02**: The refusal happens before `TcpListener::bind`, with a message naming both requirements and a distinct startup exit code.
- [ ] **BIND-03**: Loopback default behavior is unchanged; the existing `loopback_gate` tests are updated to the new rule.

### Audit
- [ ] **AUD-01**: Authorization decisions (grant, deny, expiry, device-unavailable) are logged as structured, non-sensitive events.
- [ ] **AUD-02**: Audit records contain no PIN, key material, decrypted payload, or bearer token; the log-redaction scan passes.

### Compatibility & QA
- [ ] **COMPAT-01**: A v0.2.0 client on loopback without TLS works unmodified, and the 37 frozen fixtures still match.
- [ ] **COMPAT-02**: `THREAT-MODEL.md`, `docs/SESSION-AND-LIMITS.md`, `RELEASE-NOTES.md`, and `docs/WINDOWS-SERVICE.md` describe TLS, client authentication, the tightened bind policy, and the migration path.
- [ ] **QA-01**: Nyquist sign-off for phases 3-6 is completed (`$gsd-validate-phase`), and the Linux CI fast-job test list is brought up to date.
- [ ] **QA-02**: TLS/auth tests run without a hardware token (hermetic certificates), and `cargo check --target i686-pc-windows-gnu` stays green.

---

## Future Requirements (deferred)

- Per-device locking / multi-device throughput; blocking-FFI isolation evidence on real hardware.
- Multi-vendor (FishMan / 3000GM) and Linux/macOS packaging and distribution.
- Remote log shipping (syslog/OTLP), metrics, health/readiness endpoint.
- Real certificate issuance (replace the `IssueCertificate` mock); CRL/OCSP.
- TLS for the static HTTP demo.

## Out of Scope (explicit exclusions, with reason)

- **JWT / external IdP / roles** — a static token or mTLS client certificate is sufficient for this milestone; claim-based authorization is a later concern.
- **ACME / web-PKI certificate issuance or rotation automation** — operators supply certificates.
- **Storing secrets in YAML** — the private key and bearer token come from files/env, never inline in the committed config.
- **Changing the loopback default** — TLS remains opt-in; the default stays plaintext loopback.

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| TLS-01 | Phase 7 | Pending |
| TLS-02 | Phase 7 | Pending |
| TLS-03 | Phase 7 | Pending |
| TLS-04 | Phase 7 | Pending |
| COMPAT-01 | Phase 7 | Pending |
| AUTH-01 | Phase 8 | Pending |
| AUTH-02 | Phase 8 | Pending |
| AUTH-03 | Phase 8 | Pending |
| AUTH-04 | Phase 8 | Pending |
| BIND-01 | Phase 9 | Pending |
| BIND-02 | Phase 9 | Pending |
| BIND-03 | Phase 9 | Pending |
| AUD-01 | Phase 9 | Pending |
| AUD-02 | Phase 9 | Pending |
| COMPAT-02 | Phase 10 | Pending |
| QA-01 | Phase 10 | Pending |
| QA-02 | Phase 10 | Pending |

# Requirements: LiuZX SKF Service

**Defined:** 2026-09-11
**Core Value:** 应用能够通过稳定、安全且与厂商实现解耦的统一接口访问 USB Key 的硬件密码能力。

## v1 Requirements

Requirements for milestone v0.3.0 Production Hardening. Each maps to exactly one roadmap phase.

### Foundation (FOUND)

- [x] **FOUND-01**: Developer can run `cargo test` and execute a non-zero number of Rust tests with no USB Key, no vendor driver, and no network access
- [x] **FOUND-02**: Business logic lives in a library crate so modules can be unit tested independently of the binary entry point
- [x] **FOUND-03**: The exact v0.2.0 request and response shapes for every currently supported method are captured as regression fixtures and enforced by automated tests
- [x] **FOUND-04**: SKF operations are reached through a provider abstraction with both a real vendor-backed implementation and a deterministic fake implementation
- [x] **FOUND-05**: The fake provider can inject failures (wrong PIN, missing container, missing symbol, device removal, blocking call) so failure paths are exercised in tests
- [x] **FOUND-06**: Pure encoding logic (DER, subject DN, SPKI, base64/hex) is covered by unit tests independent of hardware and OpenSSL

### Session and Authorization (SESS)

- [x] **SESS-01**: Each client connection is assigned a unique, unguessable session identity
- [x] **SESS-02**: Authorization obtained by a successful PIN verification is usable only by the session that performed it, never by another concurrent session
- [x] **SESS-03**: A session's authorization expires after a documented, configurable interval
- [x] **SESS-04**: A session's authorization is cleared when its connection closes and when the referenced device is removed
- [x] **SESS-05**: After PIN verification completes the service retains no recoverable copy of the PIN value
- [x] **SESS-06**: An operation requiring authorization fails with a stable error code when the requesting session is not authorized

### Resource Ownership (RES)

- [x] **RES-01**: Clients reference devices, applications, containers, and streaming crypto objects only through server-issued opaque identifiers
- [x] **RES-02**: The service rejects an opaque identifier that is unknown, expired, or of the wrong kind
- [x] **RES-03**: Native SKF resources owned by a session are released when the session ends, including on abrupt disconnect
- [x] **RES-04**: Native SKF resources are released on native error paths, not only on success
- [x] **RES-05**: Streaming digest state is session-scoped and cannot be observed or reused by another session

### Transport Hardening (TRANS)

- [x] **TRANS-01**: The service rejects WebSocket frames and operation payloads exceeding documented limits before allocating them
- [x] **TRANS-02**: The service bounds the number of concurrent client connections
- [x] **TRANS-03**: Long-running device operations are bounded or cancellable so a stalled token cannot block unrelated clients
- [x] **TRANS-04**: Blocking vendor calls execute off the async runtime worker threads
- [x] **TRANS-05**: Native device access is serialized per device so no two tasks use the same handle concurrently
- [x] **TRANS-06**: The service refuses to bind a non-loopback address unless explicitly opted in by configuration
- [x] **TRANS-07**: Destructive operations (delete container, import key/certificate, lock device, set label) are distinguishable from read-only operations through a documented classification

### Code Quality and CI (QUAL)

- [x] **QUAL-01**: `cargo fmt -- --check` passes for the whole repository
- [ ] **QUAL-02**: `cargo clippy --all-targets` produces no actionable warnings, with any surviving allow explicitly scoped and documented
- [ ] **QUAL-03**: A CI workflow runs formatting, linting, hardware-free tests, and an i686 Windows compile check on pull requests and main-branch pushes
- [ ] **QUAL-04**: A deliberately failing test blocks the CI check, proving the gate is enforced rather than advisory

### Windows Service Reliability (SVC)

- [ ] **SVC-01**: The service reports StartPending while initializing and Running only after the WebSocket listener is bound and the configured provider is resolved
- [ ] **SVC-02**: An initialization failure exits with a distinct non-zero service exit code so the configured restart-on-failure action actually triggers
- [ ] **SVC-03**: The installer applies restrictive permissions to the install directory so a non-administrator cannot replace the loaded executable or DLL
- [ ] **SVC-04**: Operator can install, run, upgrade over a running installation, and uninstall without leaving a stale service registration or locked files
- [ ] **SVC-05**: `status` distinguishes "process running" from "service usable" by reporting the failing or completed startup stage

### Observability and Diagnostics (OBS)

- [ ] **OBS-01**: Service logs are structured, leveled, and written to a rotating file with bounded retention
- [ ] **OBS-02**: No log output contains PIN values, private keys, session keys, or decrypted payloads
- [ ] **OBS-03**: Operator can run a local diagnostic reporting whether config loaded, provider resolved, library file present, library loaded, and listener bound
- [ ] **OBS-04**: The diagnostic reports only non-sensitive, non-identifying facts (stage booleans, provider alias, ports, uptime, error class)

### Compatibility and Release (REL)

- [ ] **REL-01**: The protocol accepts a version indicator and existing v0.2.0 clients continue to work unmodified
- [ ] **REL-02**: Methods restricted for safety remain present and return a documented error instead of disappearing
- [ ] **REL-03**: The Windows release ZIP ships with a SHA-256 checksum file
- [ ] **REL-04**: The release workflow verifies extracted package contents and that both the executable and the GM3000 DLL are PE32/i386 (`Machine=0x014C`)
- [ ] **REL-05**: Release notes document every intentional behavior change and its migration path

### Documentation (DOC)

- [ ] **DOC-01**: A threat model documents the trust boundary, the local-only exposure decision, the credential-retention decision, and known limitations
- [ ] **DOC-02**: Chinese and English documentation describe the session/authorization model, limits, and restricted methods
- [ ] **DOC-03**: Agent-facing project documentation reflects the post-refactor module layout and current commands

## v2 Requirements

Deferred beyond v0.3.0. Tracked but not in the current roadmap.

### Transport Security

- **AUTH-01**: Client authentication over a network transport
- **AUTH-02**: Encrypted remote transport with verified peer identity
- **AUTH-03**: Per-method authorization policy configuration

### Platform Expansion

- **PLAT-01**: FishMan Windows standalone package
- **PLAT-02**: 3000GM Windows standalone package
- **PLAT-03**: Linux standalone package
- **PLAT-04**: macOS standalone package

### Supply Chain

- **SUPP-01**: SBOM artifact published with each release
- **SUPP-02**: Code-signed executable and installer
- **SUPP-03**: Dependency vulnerability scanning in CI

### Dependency Modernization

- **DEPR-01**: Replace the deprecated YAML parser with a maintained alternative
- **DEPR-02**: Audit and upgrade the WebSocket/HTTP stack without behavior change

## Out of Scope

Explicitly excluded from v0.3.0. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Public internet exposure of the WebSocket API | No TLS, client authentication, or authorization model exists; loopback-only is the deliberate v0.3.0 boundary |
| Production CA / real certificate issuance in `IssueCertificate` | Compliance, key custody, and certificate policy belong to an external CA, not a device gateway |
| TLS termination library | Encrypting an unauthenticated local channel creates false assurance and a large review surface with no verified counterparty |
| Persisting authorization or PIN to disk | Would convert a temporary memory exposure into stored credentials |
| Removing or renaming v0.2.0 methods | Existing deployments have no migration path; restrict with clear errors and version instead |
| YAML parser replacement in this milestone | Separate risk class from hardening; would make regressions unattributable |
| Web framework replacement (axum/actix) | Current static-file serving is adequate; churn without risk reduction |
| Code signing | Requires purchased certificate and an external procurement process |
| Real GM3000 hardware in hosted CI | Hosted runners cannot provide the vendor driver or a physical token; hardware UAT stays a separate manual layer |
| Completing `IssueCertificate` double-certificate correctness | Mock capability; correcting it is not required for hardening and must not be presented as production-ready |

## Traceability

Which phases cover which requirements. Populated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| FOUND-01 | Phase 1 | Complete |
| FOUND-02 | Phase 1 | Complete |
| FOUND-03 | Phase 1 | Complete |
| FOUND-04 | Phase 1 | Complete |
| FOUND-05 | Phase 1 | Complete |
| FOUND-06 | Phase 1 | Complete |
| SESS-01 | Phase 2 | Complete |
| SESS-02 | Phase 2 | Complete |
| SESS-03 | Phase 2 | Complete |
| SESS-04 | Phase 2 | Complete |
| SESS-05 | Phase 2 | Complete |
| SESS-06 | Phase 2 | Complete |
| RES-01 | Phase 2 | Complete |
| RES-02 | Phase 2 | Complete |
| RES-03 | Phase 2 | Complete |
| RES-04 | Phase 2 | Complete |
| RES-05 | Phase 2 | Complete |
| TRANS-01 | Phase 3 | Complete |
| TRANS-02 | Phase 3 | Complete |
| TRANS-03 | Phase 3 | Complete |
| TRANS-04 | Phase 3 | Complete |
| TRANS-05 | Phase 3 | Complete |
| TRANS-06 | Phase 3 | Complete |
| TRANS-07 | Phase 3 | Complete |
| QUAL-01 | Phase 4 | Complete |
| QUAL-02 | Phase 4 | Pending |
| QUAL-03 | Phase 4 | Pending |
| QUAL-04 | Phase 4 | Pending |
| SVC-01 | Phase 5 | Pending |
| SVC-02 | Phase 5 | Pending |
| SVC-03 | Phase 5 | Pending |
| SVC-04 | Phase 5 | Pending |
| SVC-05 | Phase 5 | Pending |
| OBS-01 | Phase 6 | Pending |
| OBS-02 | Phase 6 | Pending |
| OBS-03 | Phase 6 | Pending |
| OBS-04 | Phase 6 | Pending |
| REL-01 | Phase 6 | Pending |
| REL-02 | Phase 6 | Pending |
| REL-03 | Phase 6 | Pending |
| REL-04 | Phase 6 | Pending |
| REL-05 | Phase 6 | Pending |
| DOC-01 | Phase 6 | Pending |
| DOC-02 | Phase 6 | Pending |
| DOC-03 | Phase 6 | Pending |

**Coverage:**
- v1 requirements: 45 total
- Mapped to phases: 45
- Unmapped: 0 ✓

---
*Requirements defined: 2026-09-11*
*Last updated: 2026-09-11 after v0.3.0 roadmap creation*

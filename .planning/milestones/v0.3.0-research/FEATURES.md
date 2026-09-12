# Feature Research — v0.3.0 Production Hardening

**Domain:** Local PKI/USB Key device gateway (Rust native-FFI service)
**Researched:** 2026-09-11
**Confidence:** HIGH for domain expectations (derived from codebase concerns and standard HSM/middleware gateway practice); MEDIUM for effort sizing

## Framing

v0.3.0 is a **hardening release**, not a feature-release. "Users" here are three distinct groups with different expectations:

1. **Integrating application developers** — call the WebSocket API from desktop/browser apps.
2. **Deployment operators / IT admins** — install, run, monitor, upgrade the Windows service.
3. **Security reviewers / integrators** — assess whether the gateway can be trusted with signing keys and PINs.

Feature expectations below are scoped to what each group assumes exists in a service that handles PKI credentials.

## Feature Landscape

### Table Stakes (Users Expect These)

Missing these makes the product feel unsafe or unmaintained, regardless of SKF functionality.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Per-connection authorization isolation | Shared authorization is a credential-confusion defect; integrators assume their PIN does not authorize other clients | HIGH | Requires session registry; touches every method branch |
| Authorization expiry and explicit clear | Operators assume a credential does not remain valid indefinitely after use | MEDIUM | TTL plus clear-on-disconnect, clear-on-device-removal |
| No plaintext PIN retained process-wide | Reviewers assume credentials are not stored recoverably | HIGH | Depends on session redesign |
| Opaque resource handles | Passing client-supplied integers as native pointers is an obvious injection/crash vector | HIGH | Handle registry with ownership and type checks |
| Deterministic native resource release | Leaked device/application/container handles degrade the token until reboot | HIGH | Scope guards plus removal-invalidation |
| Actionable provider load errors | Operators must be able to fix `LoadLibraryExW failed` without reading source | LOW | Provider, OS, path, architecture, OS error code |
| Config validation with clear failure | A typo currently surfaces at first request instead of startup | LOW | Fail-fast validation layer |
| Request size / connection / time limits | Unbounded input is a baseline expectation for any local daemon | MEDIUM | Frame, payload, connection, and per-method timeout |
| Blocking FFI off the async runtime | One slow token currently stalls unrelated connections | MEDIUM | `spawn_blocking` or provider worker pool |
| Structured, bounded logs | Operators assume logs do not fill the disk and do not contain secrets | MEDIUM | `tracing` plus rotation |
| Accurate service readiness | SCM reporting Running before bind success defeats recovery and monitoring | MEDIUM | StartPending → Running only after bind |
| Non-zero failure exit code | Without it, `sc.exe failure` restart policy never fires | LOW | Service-specific exit codes |
| Automated tests that run without hardware | Reviewers assume a change can be validated before merge | HIGH | `SkfProvider` trait plus fake |
| Regression gate in CI | Merges without any check are assumed unsafe | MEDIUM | Depends on fmt/clippy/test normalization first |

### Differentiators (Competitive Advantage)

Not required for launch, but they raise the product above typical vendor sample gateways.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Local operator health/diagnostic command | Turns "service is running but the token won't enumerate" into a one-command answer | MEDIUM | Must not leak identifiers or secrets |
| Explicit read-only vs destructive method classification | Allows a policy boundary instead of all-or-nothing access | MEDIUM | Foundation for future authorization |
| Versioned protocol negotiation | Lets v0.2.0 clients keep working while hardening lands | MEDIUM | Version field plus compatibility behavior |
| Deterministic fake-provider test suite with failure injection | Proves cleanup and error paths, which are the actual risk areas | HIGH | High leverage for confidence |
| Release checksums and content assertions | Enterprise distribution prerequisite | LOW | `SHA256SUMS` plus extracted-package verification |
| Documented threat model and trust boundary | Lets a security reviewer reach a conclusion quickly | MEDIUM | Markdown deliverable, not code |
| Local-only default enforced in code | Prevents accidental remote exposure by config typo | LOW | Refuse non-loopback bind unless explicitly opted in |
| Machine-readable error codes | Enables client-side handling instead of string matching | MEDIUM | Stable codes alongside localized messages |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| TLS/remote exposure in v0.3.0 | "So we can call it from another machine" | Without client authentication and an authorization model, TLS encrypts a channel anyone may use; creates false assurance and large review surface | Keep loopback-only; treat remote access as its own milestone with explicit auth design |
| JWT/session-token framework | Looks like standard hardening | Implies an identity model the local deployment does not have; adds a credential to protect with no verifier of trust | Per-connection authorization state derived from actual token PIN verification |
| Persisting PIN/authorization to disk | "So users don't re-enter the PIN" | Converts a memory-only exposure into persistent credential storage on disk | Session-scoped in-memory authorization with TTL, cleared on disconnect |
| Building a real CA into the service | `IssueCertificate` already exists | Compliance, key custody, and certificate policy are out of scope for a device gateway | Keep `IssueCertificate` explicitly mock; integrate external CA |
| Full parser/framework rewrite alongside hardening | Reduces perceived debt | Mixes two risk classes in one release; makes regressions unattributable | Defer `serde_yaml` replacement and any framework swap to a dedicated milestone |
| Removing or renaming v0.2.0 methods for cleanliness | "It's a breaking release" | Breaks existing deployments that have no migration path | Version and deprecate; document migration |
| `unsafe impl Send + Sync` retained without an ownership model | Avoids async plumbing work | Leaves undefined concurrency behavior with vendor DLLs | Provider worker with serialized per-device access |
| Reporting internals (serial, paths, device labels) in health output | Helpful for debugging | Leaks identifying information through an unauthenticated local surface | Report booleans and provider aliases only |
| Granting additional privileges to "fix" driver access | Fastest way to make enumeration work | Expands the blast radius of a native-FFI + parser process | Investigate the driver's actual access requirement per account, document it |

## Feature Dependencies

```
Automated CI test gate
    └──requires──> cargo fmt / clippy normalization
    └──requires──> src/lib.rs module split
                        └──requires──> SkfProvider trait boundary

Per-connection authorization isolation
    └──requires──> Session registry
                        └──requires──> src/lib.rs module split

Opaque resource handles
    └──requires──> Session registry
    └──requires──> SkfProvider trait boundary

Deterministic resource release
    └──requires──> Opaque resource handles

Request limits / timeouts
    └──enhances──> Opaque resource handles

Blocking FFI off async runtime
    └──requires──> SkfProvider trait boundary

Service readiness + failure exit codes
    └──independent──> (Windows-only, can proceed in parallel)

Structured rotating logs
    └──independent──> (can proceed early; low collision with refactor)

Release checksums + package assertions
    └──independent──> (packaging-only)

Health/diagnostic command
    └──requires──> Provider loader diagnostics
    └──enhances──> Readiness reporting

Versioned protocol negotiation
    └──enhances──> Session registry
    └──conflicts──> Removing legacy methods
```

### Dependency Notes

- **Everything testable requires the module split.** This is the single highest-leverage prerequisite; without it, no Rust test can exist.
- **Session registry is the keystone of the security requirements.** Authorization isolation, handle ownership, and expiry all depend on it.
- **The `SkfProvider` trait must land with or before the session work**, because handle ownership is only meaningful if the provider boundary owns handle lifetime.
- **CI gate is blocked by formatting/lint normalization.** Enforcing CI before normalizing guarantees a red pipeline and erodes trust in the gate.
- **Windows service correctness is separable.** Readiness and exit codes touch only `src/win_service.rs` and can land independently.
- **Logging and packaging are low-collision.** They can proceed in parallel with the refactor without serializing the milestone.
- **Versioned protocol conflicts with removing legacy methods.** Both cannot be chosen; the milestone explicitly preserves compatibility.

## MVP Definition

### Launch With (v0.3.0)

The minimum that makes this release defensibly "production hardened":

- [ ] `src/lib.rs` split with modules and a non-zero Rust test count — unlocks everything else
- [ ] `SkfProvider` trait with real and fake implementations — enables hardware-free verification
- [ ] Per-connection authorization state with TTL and clear-on-disconnect — removes the shared-credential defect
- [ ] Opaque handle registry with ownership and guaranteed release — removes client-controlled pointers
- [ ] Startup config validation with actionable provider errors — fail-fast operations
- [ ] Structured logging with rotation — bounded, non-secret logs
- [ ] Accurate SCM readiness and non-zero failure exit codes — recovery actually works
- [ ] Request/connection/time limits — baseline daemon hygiene
- [ ] CI running fmt, clippy, and hardware-free tests — merge gate
- [ ] Windows release content assertions plus `SHA256SUMS` — verifiable distribution

### Add After Validation (v0.3.x)

Add once the core hardening is proven in real deployments:

- [ ] Health/diagnostic CLI surface — trigger: first operator support request that takes more than one round trip
- [ ] Read-only vs destructive method classification — trigger: a consumer needs a restricted mode
- [ ] Versioned protocol negotiation — trigger: an external integration cannot migrate in lockstep
- [ ] Machine-readable stable error codes — trigger: a client needs programmatic error handling
- [ ] `zeroize` / memory-hygiene pass — trigger: security review requires it

### Future Consideration (v0.4+)

- [ ] Authenticated remote transport (TLS + client identity) — needs its own design milestone
- [ ] Additional provider packaging (FishMan, 3000GM, Linux, macOS) — needs per-provider hardware validation
- [ ] Dependency-migration milestone (`serde_yaml` replacement, crate modernization) — separate risk class
- [ ] SBOM and code signing — needs certificate procurement
- [ ] Multi-device concurrency and queueing strategy — needs real load data

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Module split + test count | HIGH | MEDIUM | P1 |
| `SkfProvider` trait + fake | HIGH | HIGH | P1 |
| Session authorization isolation | HIGH | HIGH | P1 |
| Opaque handle registry | HIGH | HIGH | P1 |
| Guaranteed resource release | HIGH | MEDIUM | P1 |
| Config validation | HIGH | LOW | P1 |
| Structured rotating logs | HIGH | LOW | P1 |
| SCM readiness + exit codes | HIGH | LOW | P1 |
| Request/connection limits | MEDIUM | MEDIUM | P1 |
| CI gate | HIGH | LOW (after normalization) | P1 |
| fmt/clippy normalization | MEDIUM | MEDIUM | P1 (prerequisite) |
| Blocking FFI off runtime | MEDIUM | MEDIUM | P2 |
| Release checksums + content assertions | MEDIUM | LOW | P2 |
| Health/diagnostic command | HIGH | MEDIUM | P2 |
| Destructive-method classification | MEDIUM | MEDIUM | P2 |
| Versioned protocol | MEDIUM | MEDIUM | P2 |
| Stable error codes | MEDIUM | MEDIUM | P3 |
| Threat model doc | MEDIUM | LOW | P2 |
| Warning-budget reduction | LOW | MEDIUM | P3 |

**Priority key:**
- P1: Required for v0.3.0 to claim "hardened"
- P2: Should have within v0.3.0 if budget allows
- P3: Defer unless trivially achieved

## Comparable Product Analysis

No competitor products were analyzed online during this pass. The comparison below is against the *categories* this gateway competes with, using the codebase's own gaps as the deltas.

| Capability | Vendor sample utilities | Typical PKCS#11 middleware | Our Approach (v0.3.0) |
|------------|------------------------|----------------------------|------------------------|
| API shape | Direct C calls, no IPC | Standard C API, requires C integration | JSON-RPC over local WebSocket, language-agnostic |
| Multi-vendor switching | One build per vendor | One module per vendor | Runtime provider selection from YAML |
| Credential handling | Caller-managed | Session/login handles with proper lifetime | Per-connection authorization with TTL and disconnect clearing |
| Resource handles | Raw native handles | Opaque session objects | Server-generated opaque IDs with ownership |
| Concurrency | Caller's problem | Caller's problem | Provider boundary with serialized per-device access |
| Observability | None | None | Structured logs plus local health surface |
| Testability | Manual with hardware | Manual with hardware | Fake provider CI suite plus hardware UAT |

**Positioning:** the gateway's differentiation is *integration ergonomics* (JSON-RPC, multi-vendor config, JS client). v0.3.0 must close the *trust and operability* gap that normally makes integrators reject a gateway in favor of direct PKCS#11.

## Sources

- `.planning/codebase/CONCERNS.md` — risk inventory driving table-stakes list
- `.planning/codebase/ARCHITECTURE.md` — current component boundaries
- `.planning/codebase/TESTING.md` — test gap inventory
- `.planning/PROJECT.md` — milestone goal, constraints, and out-of-scope decisions
- Direct inspection of `src/main.rs`, `src/win_service.rs`, `src/skf/*`, `packaging/windows/*`
- **No live competitor or standards research performed during this pass**

---
*Feature research for: LiuZX SKF Service v0.3.0*
*Researched: 2026-09-11*

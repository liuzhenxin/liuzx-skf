# Project Research Summary

**Project:** LiuZX SKF Service
**Domain:** Local PKI/USB Key device gateway (Rust native-FFI service, Windows deployment)
**Researched:** 2026-09-11
**Confidence:** HIGH for codebase-derived findings; MEDIUM for external version currency

## Executive Summary

LiuZX SKF Service is a local Rust gateway that exposes vendor SKF/USB Key hardware through a WebSocket JSON-RPC-style interface, most recently packaged as an i686 Windows service for the x86 GM3000 middleware. The *integration* layer is already competitive — runtime multi-vendor selection, a JavaScript client, a browser demo, and automated Windows release packaging all exceed typical vendor sample utilities. The gap is in *trust and operability*: authorization is process-wide rather than per-client, clients can supply raw native handles, native calls block the async runtime, and the entire request surface cannot be unit tested because the crate is binary-only with zero Rust tests.

The recommended approach for v0.3.0 is a strictly sequencing-sensitive hardening release. A module split into a library crate must land first, because nothing else is verifiable without it and because a security refactor on an untested codebase is the single largest risk identified in research. On top of that foundation: a `SkfProvider` trait boundary with a failure-injecting fake, per-session authorization with TTL and disconnect clearing, an opaque handle registry with deterministic release, transport limits, a normalized format/lint baseline feeding a real CI gate, accurate Windows SCM readiness with non-zero failure exit codes, and verifiable release artifacts. Deliberately excluded: TLS, a real CA, remote exposure, a parser/framework migration, and code signing.

The dominant risks are not the individual features but the *ordering* and *fidelity* of verification. Refactoring before tests, mixing structural and security changes in one commit, retargeting the provider trait as a 1:1 C ABI mirror, building a fake provider that never fails, and enabling a CI gate before the codebase is clean would each convert this release into an unverifiable rewrite. Every one of these has a concrete prevention strategy and a corresponding phase mapping.

## Key Findings

### Recommended Stack

No stack replacement is required or recommended. The existing Tokio/tungstenite/Warp/`libloading`/serde foundation carries forward unchanged. Research recommends a deliberately small set of additions: a `src/lib.rs` module split (no dependency, but the highest-leverage change in the release), the `tracing` family with rotation for bounded structured logs, a config validation layer over the existing `serde_yaml`, and `tempfile` for tests. Optional and conditional: `zeroize` (only if design cannot avoid retaining secrets), `proptest` (DER/base64 edge cases), and release checksums via `sha2` or the existing hashing stack.

**Core technologies:**
- Existing stack unchanged: Tokio 1.49, tokio-tungstenite 0.21, Warp 0.3, `libloading` 0.8, serde/serde_json/serde_yaml, `windows-service` 0.7, `windows-sys` 0.61
- `src/lib.rs` + module split: converts an untestable 3,680-line binary into testable modules — prerequisite for every other requirement
- `tracing` + `tracing-subscriber` + `tracing-appender`: replaces mixed `log`/`println!`/`eprintln!` with structured, rotating output
- `SkfProvider` trait with real + fake implementations: the single test seam and the single `unsafe` choke point
- `tempfile` (dev): guaranteed cleanup for tests that need real filesystem state

**Explicitly rejected:** TLS/rustls, JWT/OAuth frameworks, database/Redis, framework swap, `openssl` Rust bindings, and a reactive `serde_yaml` replacement.

### Expected Features

v0.3.0 is a hardening release with three distinct user groups (integrators, operators, security reviewers) whose expectations must all be met together.

**Must have (table stakes):**
- Per-connection authorization isolation with TTL and clear-on-disconnect
- No plaintext PIN retained process-wide after verification
- Opaque, server-issued, type-tagged resource handles instead of raw native pointers
- Deterministic native resource release across success, failure, disconnect, and device removal
- Actionable provider load errors (provider, OS, path, architecture, OS error)
- Startup config validation that fails fast with the offending key
- Request/connection/time limits and blocking FFI moved off async workers
- Structured, rotating, secret-free logs
- Accurate SCM readiness plus non-zero failure exit codes
- Hardware-free automated tests and a real CI merge gate

**Should have (competitive, P2):**
- Local operator health/diagnostic command that reports stage, not raw internals
- Explicit read-only vs destructive method classification
- Versioned protocol negotiation preserving v0.2.0 clients
- Release checksums plus extracted-package content assertions
- Documented threat model and trust boundary

**Defer (v0.4+):**
- Authenticated remote transport (needs its own design milestone)
- Additional provider packaging (FishMan, 3000GM, Linux, macOS)
- `serde_yaml` replacement and crate modernization as a separate risk class
- SBOM and code signing (certificate procurement)
- Multi-device concurrency/queueing strategy (needs real load data)

**Anti-features to resist:** TLS without an auth model, JWT for a local deployment, persisting authorization to disk, building a real CA, bundling a parser/framework rewrite with hardening, and removing v0.2.0 methods for cleanliness.

### Architecture Approach

The target architecture moves from a single process-wide `Arc<SkfContext>` plus a 3,000-line match into layered modules: `server` (transport, limits, readiness), `protocol` (typed DTOs, versioning, stable error codes), `session` (per-connection authorization and handle ownership), `provider` (`SkfProvider` trait, real/fake adapters, worker with serialized native access), `domain` (per-method handlers), `config` (validated `ResolvedProvider` values shared by loader and diagnostics), and `crypto` (pure DER/encoding helpers with no I/O or FFI). `src/main.rs` becomes a composition root; `src/win_service.rs` becomes a Windows-only library module.

**Major components:**
1. **`session`** — the keystone: authorization isolation, opaque handle ownership, expiry, and deterministic teardown all resolve here
2. **`provider`** — the single `unsafe` boundary and the test seam; decides what is process-scoped (loaded library) versus session-scoped (device/application/container/hash handles)
3. **`protocol`** — the only place v0.2.0 compatibility is preserved, and the natural home for version negotiation and stable error codes
4. **`server`** — owns readiness signaling and limits, which is why readiness can be fixed without touching SKF code
5. **`crypto`** — extraction of pure helpers yields immediate high-value tests with no hardware dependency

### Critical Pitfalls

1. **Refactoring a security service before any test exists** — freeze v0.2.0 protocol fixtures and land a non-zero test count *before* structural changes complete; otherwise regressions are unbisectable.
2. **Session isolation leaking through the provider layer** — deciding authorization is only half the work; process-global handle caches (as `hash_handles` is today) cause cross-session interference. Every handle needs exactly one owner.
3. **Mixing structural and security changes** — separate phases, with failing-test-first evidence for every authorization semantic change, so reviewers can attribute behavior deltas.
4. **Assuming native handles are safe to share across async tasks** — vendor DLLs must be assumed non-thread-safe; serialize per device in one worker and keep `unsafe impl Send/Sync` in exactly one documented place.
5. **Windows service reporting Running before usable, and exiting 0 on failure** — a monitoring lie plus a permanently disabled restart policy; fix readiness gating and exit codes together.
6. **Over-compliant fake provider** — a fake that always succeeds produces green CI over untested failure paths; failure injection must exist from the start, and every requirement needs at least one fake-failure test.
7. **Enabling the CI gate before normalizing fmt/clippy** — 198 formatting diffs and ~72 warnings guarantee a red pipeline and an abandoned gate.
8. **Breaking the v0.2.0 client contract** — freeze fixtures, add version negotiation, keep methods present with clear errors rather than removing them.

## Implications for Roadmap

Based on research, the following phase structure best respects discovered dependencies.

### Phase 1: Structural Foundation and Test Seam

**Rationale:** Nothing else can be verified until the crate is testable, and a security refactor on an untested codebase is the highest-risk action identified. This phase must be provably behavior-preserving.
**Delivers:** `src/lib.rs` + module split (initial extraction of pure logic: DER helpers, encoding, config, path expansion); frozen v0.2.0 protocol request/response fixtures; first Rust tests (non-zero count); `SkfProvider` trait with real adapter and failure-injecting fake.
**Addresses:** Module split, testability table stakes.
**Avoids:** Pitfall 1 (refactor without tests), Pitfall 12 (over-compliant fake), Pitfall 4 (contract break — via fixtures).

### Phase 2: Session Authorization and Resource Ownership

**Rationale:** Depends on the provider seam from Phase 1, because handle ownership is only meaningful when the provider boundary owns handle lifetime. Addresses the two most severe security concerns together.
**Delivers:** Session registry keyed by OS-CSPRNG session ID; per-session authorization `(provider, device, app) → expiry` replacing the process-wide PIN map; explicit clear on disconnect/expiry/device-removal; opaque type-tagged handle registry with ownership; deterministic release on every path; PIN bytes not retained after verification.
**Addresses:** Authorization isolation, PIN handling, opaque handles, guaranteed release, session-scoped hash state.
**Avoids:** Pitfall 2 (cross-session leakage), Pitfall 3 (mixed semantics — enforced as a sequencing rule), Pitfall 6 (zeroization theater).

### Phase 3: Transport Hardening and Concurrency

**Rationale:** Limits and timeout behavior are meaningful only once handles and sessions have defined lifetimes, since those are what must be released on disconnect or timeout.
**Delivers:** Frame/payload/connection limits enforced before allocation; per-operation duration bounds; `WaitForDevEvent` cancellation strategy; blocking FFI moved to `spawn_blocking` or a per-device worker; per-device (not global) serialization; loopback-only default enforced in code; read-only vs destructive method classification.
**Addresses:** Limits, blocking-FFI isolation, local-exposure boundary.
**Avoids:** Pitfall 8 (unbounded input/log), Pitfall 5 (unsafe handle sharing).

### Phase 4: Format/Lint Normalization and CI Gate

**Rationale:** Deliberately ordered after the heavy refactor to avoid normalizing code that is about to be restructured, and strictly before declarative CI enforcement so the first run is green.
**Delivers:** Isolated whitespace-only formatting commit verified as such; Clippy reduced to zero actionable warnings with targeted documented allows for specification-required FFI naming; CI workflow running fmt, clippy, hardware-free tests, and `cargo check --target i686-pc-windows-gnu`.
**Addresses:** CI gate, warning-budget reduction.
**Avoids:** Pitfall 9 (red CI on first run).

### Phase 5: Windows Service Reliability

**Rationale:** Independent of the Rust refactor (touches only the Windows service module and packaging) and can run in parallel with Phases 2–4, but is grouped after them here to keep the milestone describable in order. Evaluated independently for any parallel execution.
**Delivers:** StartPending with wait hint → Running only after listener bind and provider resolution; distinct non-zero service-specific exit codes per failure class; install-directory ACL hardening; full lifecycle verification including upgrade over a running service and uninstall completeness.
**Addresses:** SCM readiness, failure exit codes, install/upgrade/uninstall safety.
**Avoids:** Pitfall 7 (early Running), Pitfall 10 (unsafe lifecycle).

### Phase 6: Observability, Diagnostics, and Release Verification

**Rationale:** Logging should land as early as practical so later phases are debuggable, but the *health surface* depends on provider diagnostics from Phase 1 and the accurate readiness stages from Phase 5. Release verification closes the milestone.
**Delivers:** `tracing`-based structured logging with rotation and retention and a `RUST_LOG`-compatible filter; redacted health/diagnostic command reporting stage booleans, provider alias, ports, and uptime only; protocol version negotiation; `SHA256SUMS` plus extracted-package content and PE-architecture assertions in the release workflow; documented threat model; Chinese/English documentation and client updates.
**Addresses:** Structured logs, health surface, protocol versioning, release integrity, threat model.
**Avoids:** Pitfall 11 (leaky health), Pitfall 8 (log growth).

### Phase Ordering Rationale

- **Testability precedes everything.** Phase 1 exists because research identified "refactor before tests" as the top risk; without fixtures and a test count, later phases cannot prove non-regression.
- **Provider before session.** Handle ownership has no meaning without a provider boundary that owns handle lifetime; reversing them creates ownership definitions that must be rewritten.
- **Session before transport limits.** Timeout and disconnect behavior must release session-owned resources; limits without ownership semantics produce leaks rather than safety.
- **Normalization after the refactor, before CI enforcement.** Formatting code that is about to be restructured wastes the effort; enabling the gate before normalization guarantees abandonment.
- **Windows service work is separable.** It touches disjoint files, which makes it the natural candidate for parallel execution with Phases 2–4 despite being listed fifth.
- **Health and release verification come last.** The health surface consumes diagnostics and readiness stages built earlier; release verification must validate the finished artifact set.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 3:** Requires concrete limit values derived from real certificate/key payload sizes, plus a decision on `WaitForDevEvent` cancellation semantics that preserves current observability behavior. Also needs verification of the exact Tokio `spawn_blocking`/worker design against the existing `Language`-per-connection state.
- **Phase 5:** Requires verification of Windows ACL application behavior on the specific deployment target, the correct `ServiceExitCode` variant mapping so `sc.exe failure` actually triggers, and confirmation that vendor driver access still works under any reduced-privilege account considered.
- **Phase 6:** Requires verification of `tracing-appender` retention semantics (whether pruning is automatic or must be implemented) and of the chosen hashing approach for `SHA256SUMS` on a Windows runner.

Phases with standard patterns (skip research-phase):
- **Phase 1:** Established Rust module-extraction and trait-seam patterns; the only project-specific work is enumerating what must stay byte-identical.
- **Phase 4:** Standard rustfmt/Clippy/GitHub Actions practice; the work is normalization, not design.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH (choice) / MEDIUM (versions) | No dependency changes recommended beyond a small, well-understood set; exact crate versions were **not** verified against registries during this pass and must be resolved at plan time |
| Features | HIGH | Derived directly from the codebase risk inventory and standard gateway/HSM expectations; effort sizing is MEDIUM |
| Architecture | HIGH | Traced from actual code paths; the provider-trait and session-registry designs follow established Rust FFI/async practice |
| Pitfalls | HIGH | Grounded in observed codebase state (shared PIN map, global hash handles, early Running status, zero tests, red fmt baseline) rather than speculation |

**Overall confidence:** HIGH

### Gaps to Address

- **Exact dependency versions:** No network access during research. Resolve with `cargo add --dry-run` and `cargo tree -d` at plan time; record results in the phase plan.
- **Concrete limit values:** Frame/payload/timeout numbers must be derived from real certificate and key blob sizes observed in tests, not assumed.
- **`WaitForDevEvent` semantics:** This method intentionally blocks indefinitely; the cancellation and limit strategy needs explicit design in Phase 3 planning.
- **Vendor thread-safety ground truth:** No vendor documentation was consulted. The architecture assumes non-thread-safe and serializes; confirm whether any provider documents otherwise before relaxing that in a later milestone.
- **Cross-session interference reproduction:** The current code has no test proving the defect; Phase 2 should include a failing test first to demonstrate the problem before fixing it.
- **Windows service account feasibility:** Least-privilege service account usage is recommended but unverified against GM3000 driver requirements; treat as an investigation item in Phase 5, not a guaranteed deliverable.
- **Health surface shape:** Whether the diagnostic is a CLI subcommand, a loopback HTTP route, or both was not decided during research; resolve during Phase 6 planning.
- **Completed vs deferred scope:** Phase 6 is the largest phase and may need splitting during roadmap/plan review; the threat-model document and version negotiation are the first candidates to defer to v0.3.x if budget is tight.

## Sources

### Primary (HIGH confidence)
- `.planning/codebase/STACK.md`, `.planning/codebase/ARCHITECTURE.md`, `.planning/codebase/STRUCTURE.md`, `.planning/codebase/CONVENTIONS.md`, `.planning/codebase/TESTING.md`, `.planning/codebase/INTEGRATIONS.md`, `.planning/codebase/CONCERNS.md` — direct codebase mapping
- `.planning/PROJECT.md` — milestone goal, constraints, out-of-scope decisions
- Direct inspection of `Cargo.toml`, `src/main.rs`, `src/win_service.rs`, `src/skf/api.rs`, `src/skf/types.rs`, `api/skf_api.js`, `tests/*`, `packaging/windows/*`, `.github/workflows/release-windows.yml`

### Secondary (MEDIUM confidence)
- `.planning/research/STACK.md`, `.planning/research/FEATURES.md`, `.planning/research/ARCHITECTURE.md`, `.planning/research/PITFALLS.md` — this research pass, consistent with the codebase map
- Established Rust FFI/async integration and Windows service operational practice

### Tertiary (LOW confidence — needs validation)
- Crate version currency for `tracing`, `tracing-subscriber`, `tracing-appender`, `tempfile`, `zeroize`, `proptest`, `sha2`, `cargo-cyclonedx` — not verified against registries
- `tracing-appender` retention/pruning behavior — not verified against upstream documentation
- Vendor SKF thread-safety characteristics — not verified; assumed unsafe
- Windows `ServiceExitCode` variant mapping to `sc.exe failure` recovery triggering — not verified experimentally

---
*Research completed: 2026-09-11*
*Ready for roadmap: yes*

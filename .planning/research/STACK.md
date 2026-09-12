# Stack Research — v0.3.0 Production Hardening

**Domain:** Rust native-FFI gateway, local PKI/USB Key device access, Windows service packaging
**Researched:** 2026-09-11
**Confidence:** MEDIUM (codebase evidence is HIGH; external version currency is MEDIUM — no network access during this research pass)

## Research Constraint

No web search or documentation-fetch capability was available during this pass. Version numbers below are represented as **selection criteria plus verification commands** rather than asserted latest releases. Every added dependency must be resolved with `cargo add` and reviewed at plan time, then re-checked during execution.

## Existing Baseline (do not re-research)

- Rust 2021 binary crate `skf-service`, Tokio 1.49, tungstenite 0.21, Warp 0.3, `libloading` 0.8, `serde`/`serde_json`/`serde_yaml` 0.9, `x509-parser` 0.15, `smcrypto` 0.3, `rand` 0.8, `windows-service` 0.7, `windows-sys` 0.61.
- Node `ws` 8.19 for tests and browser demo. No Rust tests exist.
- These stay. Nothing in v0.3.0 requires replacing the WebSocket, HTTP, or SKF loading stack.

## Recommended Additions

### 1. Crate Structure: `src/lib.rs` + thin binary

**Why:** `src/main.rs` is ~3,680 lines and cannot be unit tested. Cargo cannot test a binary-only crate's internals, which is the direct cause of the zero-test baseline.

**Approach:**
- Add `src/lib.rs` exposing modules (`protocol`, `session`, `provider`, `crypto`, `config`, `service`).
- Keep `src/main.rs` as composition root only (CLI dispatch, runtime creation, listener binding).
- Keep `src/win_service.rs` as a Windows-only module of the library so it becomes testable where possible.

**Verification:** `cargo test` must run a non-zero test count while the Windows package still builds for `i686-pc-windows-msvc`.

**Confidence:** HIGH — standard Rust practice, no new dependency.

### 2. Structured Logging and Rotation

**Recommended:**
- `tracing` + `tracing-subscriber` — structured fields, level filtering, single logging facade replacing mixed `log`/`println!`/`eprintln!`.
- `tracing-appender` — non-blocking writer and daily/hourly file rotation for the Windows service log.

**Selection criteria:**
- Daily rolling file with configurable retention; never unbounded append.
- Must write to a file beside the executable in service mode and to stderr in console mode.
- Must support a `RUST_LOG`-compatible filter env var for backward compatibility.

**Migration note:** `log` remains a transitive dependency of several crates; enabling `tracing-log` or the `log` feature of `tracing-subscriber` avoids losing third-party log records.

**Verification:** generate >N days of logs synthetically, confirm old files are pruned; confirm no PIN/key material appears in output.

**Confidence:** MEDIUM — crate choice is well established, exact versions must be resolved at plan time.

### 3. Config Validation

**Recommended:** keep `serde_yaml` 0.9 (already deprecated) for v0.3.0 but add an explicit validation layer rather than a parser swap.

**Why not swap now:** A parser migration is a separate risk class from hardening, and `serde_yaml` is still functional. Swapping parsers during a security/refactor release adds review surface without addressing the actual risk (unvalidated values).

**Required validation regardless of parser:**
- Default provider alias exists in the provider map.
- Every provider has at least one path for the running OS.
- Paths are non-empty, and missing files produce an actionable startup error naming provider, OS, path, and process architecture.
- Unknown/duplicate provider or VID:PID entries are reported, not silently ignored.
- Config parse failures log the offending key path, never the whole file when secrets could be present.

**Verification:** table-driven unit tests over malformed configs, runnable in CI.

**Confidence:** HIGH for the approach; the `serde_yaml` replacement is deliberately deferred to a future milestone.

### 4. Session and Handle State

**Recommended:** no new dependency. Use the standard library plus existing Tokio primitives.

- `tokio::sync::RwLock` or `Mutex` over a session map keyed by a cryptographically random session ID.
- Session IDs generated from the OS CSPRNG via the existing `rand` crate (`rand::rngs::OsRng`), not `thread_rng`.
- Authorization state stored as a boolean/expiry per `(provider, device, application)` tuple, **not** as the PIN string.
- Handle registry mapping server-generated IDs to native handles plus ownership metadata.

**Explicit non-choice:** do not add a general-purpose auth/JWT crate. There is no network auth requirement in v0.3.0 (local-only scope), and adding a token framework would imply security properties the transport does not provide.

**Zeroization (optional, evaluate at plan time):** `zeroize` for any buffer that must briefly hold PIN or key bytes. Only worth adding if the design cannot avoid retaining such bytes at all.

**Confidence:** HIGH.

### 5. Test Infrastructure

**Recommended:**
- `tokio` test macros (already available via the `full` feature) for async unit/integration tests.
- `#[cfg(test)]` modules for pure logic — no new dependency.
- `tests/*.rs` integration tests that start the server on an **ephemeral port** (`127.0.0.1:0`) with a fake provider.
- `tempfile` for tests that need real temporary directories and guaranteed cleanup.
- `proptest` or `quickcheck` **optional** — valuable for DER/base64/hex round-trip edge cases, but only adopt if the phase has budget.

**Fake provider design:**
- Introduce a `SkfProvider` trait mirroring the subset of SKF operations the service uses.
- Real implementation wraps the existing `SkfApi`.
- Fake implementation returns deterministic handles, configurable failures, and recorded call sequences.
- The fake must be able to simulate: device absent, PIN wrong, PIN locked, container missing, symbol missing, device removed mid-operation, and slow/blocking calls.

**Verification:** `cargo test` passes with no token, no driver, and no network.

**Confidence:** HIGH for approach; `proptest` adoption is MEDIUM (budget-dependent).

### 6. Windows Service Readiness and Health

**Recommended:** no new crate. Extend `src/win_service.rs` and the existing `windows-service` usage.

- Report `StartPending` with a wait hint during initialization.
- Report `Running` only after the WebSocket listener has successfully bound.
- Report a non-zero `ServiceExitCode::ServiceSpecific(...)` on initialization failure so `sc.exe failure` recovery actually triggers.
- Add a local-only diagnostic surface (CLI `status`/`health` subcommand and/or a loopback-only health HTTP route) reporting: config loaded, provider resolved, library file present, library load attempted/succeeded, listener bound, last error, uptime.

**Security constraint:** the health surface must not report PINs, key material, device serial numbers, or absolute paths that leak user identity. Report provider alias and boolean/file-existence facts only.

**Confidence:** HIGH.

### 7. Release Integrity

**Recommended:**
- `sha2` (or the existing hashing stack) to generate a `SHA256SUMS` file in the Windows build script — no external tooling required.
- `cargo-cyclonedx` **optional** for an SBOM artifact; only if the phase budget allows and the output is actually consumed.
- Keep the existing PE Machine (`0x014C`) assertions as a hard release gate and extend them with an extracted-package content assertion (expected files present, DLL architecture correct).

**Explicit non-choice:** code signing requires a purchased certificate and is out of scope; the build must leave a clearly documented insertion point for it.

**Confidence:** HIGH for checksums and content assertions; MEDIUM for SBOM adoption.

### 8. CI Enforcement

**Recommended additions to `.github/workflows/` (or a shared workflow):**
- A `tests` workflow on push/PR running `cargo fmt -- --check`, `cargo clippy -- -D warnings` (or a documented warning budget), `cargo test` on Linux, and `cargo check --target i686-pc-windows-gnu`.
- Keep `release-windows.yml` tag-triggered and add a package smoke step that extracts the ZIP and asserts expected contents.

**Prerequisite:** formatting and Clippy must be normalized first (198 fmt diffs, ~72 Clippy warnings today), otherwise the gate fails on day one. This creates a hard ordering dependency for the roadmap.

**Confidence:** HIGH.

## Recommended Stack Additions Summary

| Addition | Purpose | New Dependency | Priority |
|----------|---------|----------------|----------|
| `src/lib.rs` + module split | Make logic testable | none | P1 |
| `tracing` + `tracing-subscriber` + `tracing-appender` | Structured, rotating logs | yes (3) | P1 |
| Config validation layer | Fail fast with actionable errors | none | P1 |
| Session + handle registry | Remove shared PIN and raw pointer exposure | none | P1 |
| `SkfProvider` trait + fake | Hardware-free CI tests | none | P1 |
| `tempfile` | Safe temp-file handling in tests | yes (dev) | P2 |
| Health/readiness surface | Operator diagnosis, correct SCM states | none | P2 |
| `zeroize` | Secret memory hygiene | yes | P2 (conditional) |
| SHA-256 release checksums | Artifact verification | `sha2` or existing | P2 |
| `proptest` | DER/base64 edge-case fuzzing | yes (dev) | P3 |
| `cargo-cyclonedx` | SBOM | yes (tool) | P3 |
| Code signing | Artifact authenticity | certificate | Out of scope |

## What NOT to Add

| Rejected | Reason |
|----------|--------|
| TLS termination library (rustls/native-tls) | Local-only scope; adding transport encryption without an auth model creates false assurance |
| JWT/OAuth framework | No remote identity requirement; implies guarantees the design does not provide |
| Database / Redis | State is process-local by design; persistence adds deployment surface with no v0.3.0 requirement |
| Full framework swap (axum/actix) | Warp already serves static files adequately; replacing it is churn without risk reduction |
| `openssl` Rust bindings | Would couple the whole service to OpenSSL for a mock-only feature; keep the CLI invocation isolated |
| Unmaintained/renamed YAML parser adopted reactively | Deferred deliberately; separate milestone with its own compatibility testing |

## Version Verification Commands

Run at plan time and record results in the phase plan:

```bash
cargo add tracing tracing-subscriber tracing-appender --dry-run
cargo add --dev tempfile --dry-run
cargo tree -d          # check for duplicate/conflicting versions
cargo deny check       # only if cargo-deny is adopted
```

## Sources

- Codebase map: `.planning/codebase/STACK.md`, `.planning/codebase/CONCERNS.md`, `.planning/codebase/TESTING.md`
- Direct inspection of `Cargo.toml`, `src/main.rs`, `src/win_service.rs`, `src/skf/*`, `tests/*`, `packaging/windows/*`
- Established Rust service-hardening practice (trait-based provider seams, `spawn_blocking` for FFI, tracing stack)
- **Not verified against live crate registries or upstream docs during this pass**

---
*Stack research for: LiuZX SKF Service v0.3.0*
*Researched: 2026-09-11*

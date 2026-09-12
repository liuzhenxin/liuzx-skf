# Codebase Concerns

**Analysis Date:** 2026-09-11

## Critical Security Concerns

**Unauthenticated privileged hardware API:**
- Risk: Any process that can connect can verify PINs, sign data, import keys/certificates, delete containers, relabel/lock devices, and transmit APDUs.
- Files: Listener and dispatch in `src/main.rs`; client surface in `api/skf_api.js`.
- Current mitigation: WebSocket defaults to loopback, and Windows service HTTP also defaults to loopback.
- Gap: `SKF_WS_ADDR=0.0.0.0:9001` is documented for remote use without adding TLS, authentication, authorization, or origin controls.
- Recommendation: Add an authenticated local IPC or mutually authenticated transport; explicitly classify destructive methods and deny remote exposure by default.

**Process-wide plaintext PIN cache:**
- Risk: A PIN verified by one WebSocket client authorizes later operations from every client because one `Arc<SkfContext>` is shared process-wide.
- Files: `SkfContext.pins`, `spawn_ws_client()`, and PIN-consuming branches in `src/main.rs`.
- Current mitigation: Cache is memory-only and disappears on restart; values are no longer logged.
- Gap: No per-session ownership, expiration, zeroization, attempt policy, or guaranteed clearing on disconnect/device removal.
- Recommendation: Store authorization tokens per connection, never retain recoverable PIN strings longer than verification requires, add TTL and explicit logout/clear semantics, and use zeroizing memory where unavoidable.

**Untrusted raw native handles:**
- Risk: Methods accept numeric/string handles from clients and cast them back to pointers passed into vendor DLLs; invalid values can crash the service or exercise undefined vendor behavior.
- Files: `ConnectDev`, `DisConnectDev`, `GenerateRandom`, and streaming hash methods in `src/main.rs`; `SendHandle` in `src/skf/types.rs`.
- Current mitigation: None beyond loopback binding.
- Recommendation: Return opaque server-generated IDs backed by a validated per-session handle registry; enforce type, owner, lifetime, and cleanup checks.

**FFI memory-safety assumptions:**
- Risk: `CStr::from_ptr` can scan beyond fixed vendor buffers if strings are not NUL-terminated, and `unsafe impl Send/Sync` assumes vendor handles are thread-safe.
- Files: `GetDevInfo` branch in `src/main.rs`; `SendHandle` in `src/skf/types.rs`.
- Current mitigation: C-layout types and mostly specification-shaped calls.
- Recommendation: Decode fixed buffers with bounded NUL search, centralize unsafe conversions, document vendor thread guarantees, and serialize access where guarantees are absent.

**LocalSystem service privilege:**
- Risk: A network-facing parser and third-party DLL execute as LocalSystem after installation.
- Files: account selection in `src/win_service.rs`; installer in `packaging/windows/install.ps1`.
- Current mitigation: Default network bind is loopback in service mode.
- Recommendation: Use a dedicated least-privilege service account where driver access permits, apply directory ACLs, and threat-model loaded DLL replacement.

## Functional Bugs and Correctness Risks

**Double-certificate encryption certificate does not match generated encryption key:**
- Symptoms: Returned encryption certificate may not correspond to the private key packed into `encPriKey`, undermining real dual-certificate installation.
- Trigger: Call `IssueCertificate` with `double=true`.
- File: `IssueCertificate` branch in `src/main.rs` explicitly generates a separate temporary OpenSSL key and notes the mismatch in comments.
- Workaround: Treat `IssueCertificate` as mock-only and use a real CA flow for production.
- Fix: Build/sign the encryption certificate with the actual generated SM2 public key and add independent key/certificate matching tests.

**Service reports healthy too early and failures as successful stop:**
- Symptoms: SCM can show Running before config parsing/listener binding completes; startup failure may end with exit code zero, preventing configured recovery.
- File: `run_service()` in `src/win_service.rs`.
- Root cause: Running is reported before `run_server()` succeeds, and Stopped always uses `Win32(0)`.
- Fix: Report StartPending during initialization, Running only after readiness, and a nonzero service-specific exit code on failure.

**RSA verification test is not meaningful:**
- Symptoms: Test supplies the signature bytes as `pubKeyBase64` placeholder.
- File: `tests/test_all_apis.js` RSA verification section.
- Impact: Regressions in `RSAVerify` can pass or be misclassified.
- Fix: Export/use the actual `RSAPUBLICKEYBLOB` and verify positive and tampered-data cases.

**Protocol is JSON-RPC-like, not JSON-RPC 2.0 compliant:**
- Symptoms: Requests do not validate a `jsonrpc` field; responses omit `jsonrpc` and use a numeric `error` field even on success.
- Files: `RpcRequest` and `RpcResponse` in `src/main.rs`.
- Impact: Standard JSON-RPC clients and tooling cannot interoperate reliably.
- Fix: Either formalize the custom protocol with versioning or implement the JSON-RPC 2.0 request/result/error schema with compatibility handling.

## Technical Debt

**Monolithic dispatcher:**
- Issue: `src/main.rs` is about 3,680 lines and `handle_request()` contains 37 method branches with duplicated provider/device/application/container/PIN lifecycle code.
- Impact: High change collision, inconsistent validation/cleanup/localization, and poor unit-testability.
- Fix approach: Introduce `src/lib.rs`, typed protocol DTOs, domain handler modules, and RAII wrappers for native resources.

**Synchronous native and process work inside async handlers:**
- Issue: SKF calls and OpenSSL subprocesses execute directly in Tokio tasks; `WaitForDevEvent` can block indefinitely.
- Files: `handle_request()` in `src/main.rs`.
- Impact: A slow token or driver can consume runtime workers and delay unrelated clients.
- Fix approach: Use `spawn_blocking` or dedicated per-provider workers with bounded queues and cancellation.

**Ad hoc request validation:**
- Issue: Each branch manually indexes positional `serde_json::Value` parameters.
- Impact: Repeated code, weak type constraints, inconsistent errors, and no payload limits.
- Fix approach: Define typed request structs/schemas, shared validation, method metadata, and maximum frame/data sizes.

**Mixed logging and append-only files:**
- Issue: `log`, `println!`, and unconditional debug `eprintln!` are mixed; Windows log file never rotates.
- Files: `src/main.rs`, `src/win_service.rs`.
- Impact: Noisy production output and unbounded disk growth.
- Fix approach: Standardize structured logging, configure rotation/retention, and add correlation IDs without sensitive material.

**Formatter/lint baseline:**
- Issue: `cargo fmt -- --check` reports 198 diff sections; Clippy emits roughly 72 warnings per target.
- Files: primarily `src/main.rs`, `src/skf/api.rs`, and FFI acronym types.
- Impact: Review noise and no enforceable style gate.
- Fix approach: Normalize formatting in an isolated commit, explicitly allow specification-required FFI names, resolve actionable Clippy warnings, then enforce CI.

**Documentation drift:**
- Issue: `AGENTS.md`/`CLAUDE.md` describe outdated DLL paths, line counts, and context ownership; `README_CN.md` references test files that do not exist.
- Impact: Operators and future agents follow incorrect setup/testing instructions.
- Fix approach: Generate one authoritative API/ops reference from code or add documentation verification to release checks.

## Reliability and Resource Ownership

**Handle lifecycle is not connection-scoped:**
- Problem: Device and streaming hash handles can outlive the connection that created them; hash state is process-global.
- Files: `SkfContext.hash_handles` and connection loop in `src/main.rs`.
- Impact: Leaks, cross-client interference, and stale handles after device removal.
- Improvement: Add per-session registries with drop-based cleanup and provider/device removal invalidation.

**Library calls are inconsistently cached:**
- Problem: Most paths use `SkfContext.get_api()`, while some lock/transmit/hash paths reload libraries directly from `lib_path`.
- Files: lower method branches in `src/main.rs`.
- Impact: Inconsistent symbol/library lifetime assumptions and duplicated error behavior.
- Improvement: Route all operations through a single provider runtime/worker abstraction.

**No readiness or health interface:**
- Problem: Port/process status does not prove config, DLL, driver, and device operation are healthy.
- Impact: Installers and monitoring cannot distinguish running from usable.
- Improvement: Add local health/readiness diagnostics with provider load checks and explicit degraded states that do not expose secrets.

**Unbounded concurrency and payloads:**
- Problem: No connection limit, request timeout, frame limit, operation queue, or cryptographic payload cap is configured.
- Impact: Local denial of service and memory pressure; external exposure greatly increases risk.
- Improvement: Add conservative limits and backpressure before supporting remote clients.

## Dependencies at Risk

**`serde_yaml 0.9.34+deprecated`:**
- Risk: The resolved package is explicitly marked deprecated.
- Impact: Future maintenance/security updates may stop.
- Migration plan: Evaluate a maintained YAML parser or a simpler validated config format, preserving backward compatibility.

**Vendor binaries:**
- Risk: Closed binary dependencies have platform/architecture, licensing, driver, and provenance constraints.
- Impact: Runtime crashes or supply-chain exposure cannot be corrected in Rust code.
- Mitigation: Record checksums/versions, verify signatures where available, restrict write ACLs, and test each supported driver matrix.

## Missing Critical Capabilities

**Deterministic non-hardware test seam:**
- Problem: Core workflows cannot be regression-tested in CI without a physical token.
- Blocks: Safe refactoring, protocol hardening, concurrency fixes, and broad pull-request CI.
- Complexity: Medium to high; requires a provider trait/fake backend and modular dispatch.

**Production Windows service UAT:**
- Problem: CI builds the ZIP but does not test install/start/readiness/stop/restart/uninstall.
- Blocks: Confidence that releases work after installation and reboot.
- Complexity: Medium; requires an elevated self-hosted or disposable Windows environment, with hardware checks separated from SCM checks.

**Release integrity and provenance:**
- Problem: ZIP is published without an accompanying checksum file, executable signing, SBOM, or documented vendor-binary provenance.
- Blocks: Enterprise verification and controlled deployment.
- Complexity: Medium, depending on code-signing certificate availability.

## Test Coverage Gaps

**Pure Rust logic:**
- What's not tested: Config/env expansion, DER helpers, request parsing, error mapping, and service state logic.
- Priority: High.
- Difficulty: Low after extracting code from the binary entry point.

**Native boundary failures:**
- What's not tested: Missing symbols, malformed buffers, invalid handles, device removal mid-operation, concurrency, and cleanup.
- Priority: Critical.
- Difficulty: High without a fake native backend.

**Security boundaries:**
- What's not tested: Cross-client PIN reuse, unauthorized destructive calls, oversized frames, and remote exposure.
- Priority: Critical.
- Difficulty: Medium after session and transport abstractions exist.

**Packaging/service lifecycle:**
- What's not tested: Installed-path ACLs, reboot auto-start, failure recovery, log growth, upgrade, and uninstaller completeness.
- Priority: High.
- Difficulty: Medium; platform-specific automation required.

---

*Concerns audit: 2026-09-11*
*Update as risks are mitigated or new provider matrices are introduced*

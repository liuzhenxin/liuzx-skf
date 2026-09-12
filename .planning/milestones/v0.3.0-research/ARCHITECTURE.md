# Architecture Research — v0.3.0 Production Hardening

**Domain:** Rust native-FFI gateway with async transport and dynamic vendor libraries
**Researched:** 2026-09-11
**Confidence:** HIGH (derived from direct codebase inspection and standard Rust FFI/async integration patterns)

## Current Architecture (Baseline)

```
main()  ──> run_server(shutdown)  ──> TCP accept loop ──> spawn_ws_client()
                   │                                            │
                   │                                            └──> handle_request(&SkfContext, json, &mut Language)
                   │                                                       │
                   └──> warp::serve(api/)                                  ├──> SkfContext.get_api(provider)  [RwLock<HashMap<alias, Arc<SkfApi>>>]
                                                                           ├──> SkfContext.pins               [RwLock<HashMap<key, PIN string>>]
                                                                           ├──> SkfContext.hash_handles       [RwLock<HashMap<key,(HANDLE,DEVHANDLE,..)>>]
                                                                           ├──> DER helpers / smcrypto / OpenSSL CLI
                                                                           └──> SkfApi ──> libloading ──> vendor DLL (raw C ABI)
```

**Load-bearing problems:**

1. One `Arc<SkfContext>` is process-wide, so authorization state is shared across all connections.
2. `handle_request` is a single ~3,000-line match; logic cannot be unit tested because the crate is binary-only.
3. Client input flows into native handle casts; there is no ownership or type validation.
4. Native calls execute directly on Tokio worker threads.
5. `win_service` reports Running before the listener is bound and returns exit code 0 on failure.

## Target Architecture

```
main.rs  (composition root: CLI dispatch, mode selection, runtime creation)
   │
   ├── service/          Windows SCM lifecycle, readiness, exit codes
   ├── server/           Transport: TCP bind, WS upgrade, HTTP static, limits, shutdown
   ├── protocol/         Request/response DTOs, versioning, error codes, validation
   ├── session/          Per-connection identity, authorization state, TTL, handle ownership
   ├── domain/           Method handlers composed from provider + session
   ├── provider/         SkfProvider trait, real adapter, fake adapter, worker, handle registry
   ├── config/           Parse + validate provider/path/OS configuration
   └── crypto/           Pure DER/encoding/CSR/ENVELOPE helpers (no I/O, no FFI)
```

Compilation units: `src/lib.rs` (all modules above) plus `src/main.rs` (binary that calls into the library) plus `src/win_service.rs` (Windows-only library module). This single change is what makes every later phase verifiable.

## Component Boundaries

### Transport Layer (`server/`)

- **Owns:** TCP listener, WebSocket upgrade, HTTP static route, shutdown signal, connection accounting.
- **Must not:** know about SKF, providers, PINs, or handle types.
- **Exposes:** `Server::serve(addr, shutdown, session_factory) -> Result<()>`, readiness signal once bound.
- **Rationale:** Readiness reporting and limits both live here; keeping SKF out makes them independently testable.

### Protocol Layer (`protocol/`)

- **Owns:** deserialization, method registry, parameter extraction/validation, response formatting, protocol version, stable error codes.
- **Must not:** call the provider or hold handles.
- **Exposes:** typed request enum and a handler dispatch table.
- **Rationale:** Replaces 37 hand-written `params.get(n)` branches with testable typed parsing; makes the version negotiation point explicit.

### Session Layer (`session/`)

- **Owns:** session ID (OS CSPRNG), per-session authorization state with expiry, per-session resource-handle table, cleanup on disconnect/expiry.
- **Must not:** store recoverable PIN strings after verification completes; must not outlive a connection.
- **Exposes:** `Session::authorize(provider, device, app)`, `Session::is_authorized(...)`, `Session::register_handle(...) -> OpaqueId`, `Session::resolve_handle(id, expected_kind)`.
- **Rationale:** This is the keystone. Authorization isolation, opaque handles, and deterministic release all resolve here.

### Provider Layer (`provider/`)

- **Owns:** the `SkfProvider` trait, the real `SkfApi`-backed adapter, the deterministic fake, the worker/queue that serializes native access, and native handle lifetime.
- **Must not:** expose raw pointers across its boundary; must not know about WebSocket or JSON.
- **Exposes:** `Provider::load(config, alias) -> Result<Box<dyn SkfProvider>>`, plus per-operation methods returning typed results and error codes.
- **Rationale:** Single choke point for `unsafe`, the seam for hardware-free tests, and where serialization/`spawn_blocking` is implemented once instead of per method.

### Domain Layer (`domain/`)

- **Owns:** method semantics — certificate workflows, digest/sign/encrypt flows, container management.
- **Must not:** parse JSON, bind sockets, or hold raw native handles across await points.
- **Exposes:** one handler per method, each taking a typed request and a session reference.
- **Rationale:** Converts an untestable 3,000-line match into per-method units with focused tests.

### Config Layer (`config/`)

- **Owns:** load, expand `%VAR%`, validate, and report diagnostics.
- **Exposes:** validated `ResolvedProvider { alias, os, path, exists }` values so both the loader and the health surface share one source of truth.

### Crypto Layer (`crypto/`)

- **Owns:** pure functions — DER encoding, subject DN parsing, SPKI building, ENVELOPE assembly, base64/hex helpers.
- **Must not:** perform file I/O, spawn processes, or call FFI.
- **Rationale:** Currently these helpers are interleaved with OpenSSL subprocess calls in `main.rs`; separating them yields immediate high-value unit tests.

## Data Flow (Target)

**Request:**
1. `server` accepts a connection and creates a `Session` from the session registry.
2. Frame is size-checked against limits; oversized frames close the connection with a protocol error.
3. `protocol` deserializes into a typed request and validates parameters, producing either a typed call or a structured error.
4. `domain` handler receives the typed request plus `&Session`.
5. Handler checks `Session::is_authorized`; if not authorized it either rejects or performs a verification it can prove succeeded.
6. Handler resolves any referenced opaque handle through the session, validating kind and ownership.
7. Handler calls `provider`, which marshals into a worker/`spawn_blocking` and performs the native call.
8. Provider returns a typed result; handler encodes the response value.
9. `protocol` formats the response with a stable error code; `server` writes the frame.

**Authorization:**
1. `CheckPIN` resolves provider/device/application and calls the provider's VERIFY.
2. On success the session records an authorization entry `(provider, device, app) -> expires_at`; the PIN byte buffer is released immediately.
3. Subsequent privileged operations consult the session; no other connection can satisfy the check.
4. Expiry, disconnect, device removal, and explicit clear all invalidate the entry and release every handle the session owns.

**Shutdown:**
1. SCM stop or SIGINT flips the shutdown watch.
2. `server` stops accepting, then closes active sessions with a bounded grace period.
3. Session teardown releases all owned native handles through the provider.
4. Service reports Stopped with an accurate exit code.

## Key Abstractions

**`SkfProvider` (trait)**
- Purpose: isolate every native call behind one seam.
- Implementations: `NativeSkfProvider` (wraps `SkfApi`), `FakeSkfProvider` (tests).
- Pattern: Adapter + trait object, with the worker owning serialization.

**`Session`**
- Purpose: per-connection identity, authorization, and handle ownership.
- Pattern: Registry-managed lifecycle with drop-based cleanup.

**`OpaqueHandle`**
- Purpose: replace client-supplied native pointers with server-issued IDs.
- Pattern: Generational index or UUID keyed lookup; type-tagged so a container ID cannot be used as a device ID.

**`ResolvedProvider`**
- Purpose: one validated description of a provider's loadable library.
- Pattern: Value object produced by config validation and consumed by both the loader and diagnostics.

**Stable error codes**
- Purpose: allow programmatic client handling and correct localization at one point.
- Pattern: Enum with numeric codes plus localized message rendering.

## Build Order Implications

Dependencies discovered during research, in safe build order:

1. **Module split (`src/lib.rs`)** — no other phase can be verified without it.
2. **Provider trait + fake** — must exist before session handle ownership has meaning.
3. **Session registry + authorization** — depends on 2.
4. **Opaque handles + guaranteed release** — depends on 2 and 3.
5. **Limits/timeouts + blocking-FFI isolation** — depends on 1 and 2 (transport and provider seams).
6. **fmt/clippy normalization → CI gate** — the gate must come after normalization.
7. **Windows readiness/exit codes** — independent; can run early or in parallel.
8. **Structured logging + rotation** — independent; should land early so later phases are debuggable.
9. **Config validation + health surface** — depends on 1; health depends on provider diagnostics.
10. **Release checksums + content assertions** — packaging-only; independent.
11. **Protocol versioning** — depends on 1 and 3.

**Parallelization guidance:** 7, 8, and 10 touch disjoint files from 2–5 and can run concurrently. 2–5 are inherently sequential. 6 must follow 1 and the normalization work but precede any "green CI" success criterion. 9's health surface should follow the provider diagnostics added in 7.

## Error Handling Architecture

- **Boundary:** `anyhow` for startup/operational failures with context.
- **Domain:** typed errors mapped to stable protocol codes; native return codes preserved as hex detail.
- **FFI:** single conversion point from `ULONG` return code to typed error; no raw code leaks past the provider.
- **Cleanup:** RAII/scope guards so every error path releases handles — currently the dominant source of duplicated cleanup blocks.

## Cross-Cutting Concerns

**Logging:** one `tracing` setup; no `println!`/`eprintln!` in library code. Correlation ID per connection, never per-secret.

**Cancellation:** long-running/blocking operations must be cancellable or bounded; `WaitForDevEvent` specifically needs a documented strategy since it blocks indefinitely by design.

**Platform:** Windows modules stay `#[cfg(windows)]`; the library must compile and test on Linux/macOS for CI.

**Compatibility:** the protocol layer is the single place where v0.2.0 compatibility is preserved; domain and provider layers should not carry legacy baggage.

## Architecture Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| Refactor lands before tests exist | Regressions become unattributable | Land module split with a minimal pure-logic test suite first; expand as layers move |
| Session registry becomes a new global | Repeats the shared-state defect | Registry keyed by session ID with explicit teardown; no cross-session lookups in domain code |
| Provider trait becomes a 1:1 mirror of the C ABI | No testability gain, just indirection | Design around domain operations, not per-symbol wrappers |
| FFI serialization becomes a global lock | Throughput collapse with multiple tokens | Serialize per device/provider instance, not per process |
| Windows-only logic leaks into shared modules | Linux CI can no longer run tests | Keep `#[cfg(windows)]` at the service module boundary |

## Sources

- `.planning/codebase/ARCHITECTURE.md`, `.planning/codebase/STRUCTURE.md`, `.planning/codebase/CONCERNS.md`
- Direct inspection of `src/main.rs`, `src/win_service.rs`, `src/skf/api.rs`, `src/skf/types.rs`
- Standard Rust FFI/async integration practice (trait seam, `spawn_blocking` for blocking FFI, RAII resource ownership)
- **No upstream documentation fetch performed during this pass**

---
*Architecture research for: LiuZX SKF Service v0.3.0*
*Researched: 2026-09-11*

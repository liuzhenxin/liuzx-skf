# Architecture

**Analysis Date:** 2026-09-11

## Pattern Overview

**Overall:** Single-process asynchronous gateway with a dynamic native-plugin boundary.

**Key Characteristics:**
- One Rust executable hosts WebSocket JSON-RPC-style request handling and a static HTTP demo server.
- Vendor support is configured by alias and implemented through lazily loaded SKF shared libraries.
- Most orchestration and business logic resides in one large request dispatcher in `src/main.rs`.
- Cryptographic state and credentials are held in shared process memory; there is no database.
- The same executable supports foreground and Windows SCM service modes.

## Conceptual Layers

**Process and Hosting Layer:**
- Purpose: Select console or Windows service mode, create Tokio runtime, bind listeners, and handle shutdown.
- Contains: `main()`, `run_server()`, `spawn_ws_client()` in `src/main.rs`; SCM lifecycle in `src/win_service.rs`.
- Depends on: Tokio, Warp, tungstenite, and Windows service APIs.
- Used by: Operators, Windows SCM, WebSocket clients, and browser demo users.

**Protocol and Dispatch Layer:**
- Purpose: Parse incoming request JSON, maintain per-connection language choice, route method names, and format responses.
- Contains: `RpcRequest`, `RpcResponse`, `Language`, and `handle_request()` in `src/main.rs`.
- Depends on: Shared `SkfContext`, certificate helpers, base64, and all SKF wrappers.
- Used by: Each WebSocket connection task.

**Application Workflow Layer:**
- Purpose: Compose device, application, container, certificate, PIN, hashing, signing, and encryption operations.
- Contains: The method match arms inside `handle_request()` in `src/main.rs`.
- Depends on: SKF API wrappers, in-memory PIN/hash state, DER helpers, OpenSSL, and `smcrypto`.
- Used by: JSON-RPC-style method dispatch.

**Provider Context Layer:**
- Purpose: Load YAML configuration, resolve platform library paths, expand `%VAR%`, cache loaded APIs, PINs, and streaming hash handles.
- Contains: `SkfConfig` and `SkfContext` in `src/main.rs`.
- Depends on: serde_yaml, `RwLock<HashMap<...>>`, libloading.
- Used by: All request handlers.

**Native SKF Adapter Layer:**
- Purpose: Define the vendor-neutral Rust-facing wrapper over the SKF C ABI.
- Contains: Function pointer aliases and `SkfApi` methods in `src/skf/api.rs`.
- Depends on: C-compatible definitions in `src/skf/types.rs` and vendor shared-library symbols.
- Used by: Application workflow code.

**FFI Type Layer:**
- Purpose: Mirror SKF scalar types, opaque handles, return codes, algorithm IDs, and C-layout structures.
- Contains: `src/skf/types.rs`.
- Depends on: Rust FFI primitives only.
- Used by: `src/skf/api.rs` and `src/main.rs`.

**Client and Packaging Layer:**
- Purpose: Offer JavaScript consumption, a browser demo, integration scripts, and Windows distribution automation.
- Contains: `api/`, `tests/`, `packaging/windows/`, and `.github/workflows/release-windows.yml`.

## Data Flow

**Typical WebSocket Request:**
1. A client connects to the TCP listener created by `run_server()` in `src/main.rs`.
2. `spawn_ws_client()` upgrades the stream and creates a per-connection `Language` value.
3. Text frames are deserialized into `RpcRequest`; malformed JSON returns error `-1`.
4. `handle_request()` matches the method string and extracts positional parameters manually.
5. The handler resolves a provider through `SkfContext` and lazily loads/caches its native library.
6. The handler opens native device/application/container handles, performs SKF calls, and usually closes temporary handles.
7. Binary outputs are encoded as base64 or hex and returned in `RpcResponse`.
8. The response is serialized and sent on the same WebSocket.

**Windows Service Startup:**
1. SCM launches `skf-service.exe --service`.
2. `src/win_service.rs` redirects standard streams, registers a control handler, and creates a watch channel.
3. A dedicated Tokio runtime calls the same `run_server()` used by console mode.
4. Stop or shutdown controls flip the watch value and cause the WebSocket accept loop to exit.
5. Service status is reported as stopped after the runtime exits.

**Certificate Creation:**
1. `CreatePKCS10` opens the requested token application/container and creates an SM2 or RSA key pair.
2. DER helper functions in `src/main.rs` build subject and public-key structures.
3. The hardware token signs request data and the service returns PEM CSR content.
4. Mock `IssueCertificate` writes temporary files and invokes OpenSSL to create demonstration certificates.
5. Double-certificate mode additionally uses `smcrypto` to construct an `ENVELOPEDKEYBLOB` payload.

**State Management:**
- One `Arc<SkfContext>` is created per process and shared by all WebSocket connections.
- Loaded provider libraries are cached for process lifetime in `SkfContext.apis`.
- Verified PIN strings are cached by provider/device/application key in `SkfContext.pins`.
- Streaming hash handles are cached by string handle key in `SkfContext.hash_handles`.
- Connection language is the only state scoped to an individual WebSocket.
- No state is persisted across process restart except mock CA files in the OS temporary directory.

## Key Abstractions

**`SkfContext`:**
- Represents process-wide provider configuration and mutable runtime caches.
- Pattern: Shared service context using `Arc` plus `RwLock`.

**`SkfApi`:**
- Owns an `Arc<Library>` and resolves each SKF symbol on demand.
- Pattern: Dynamic adapter/facade over a C ABI.

**`SendHandle`:**
- Wraps raw native handles and unsafely declares them `Send + Sync` for async/shared state.
- Pattern: FFI escape hatch whose safety depends entirely on vendor behavior and caller discipline.

**JSON-RPC Method Name:**
- Method strings such as `EnumDevice`, `CreatePKCS10`, and `DigestInit` select inline handler branches.
- Pattern: Large command dispatcher rather than typed route modules.

**Provider Alias:**
- Logical name such as `GM3000` maps to platform-specific shared-library paths.
- Pattern: Configuration-driven native plugin selection.

## Entry Points

**Service Executable:**
- Location: `src/main.rs`.
- Triggers: Console execution, service management subcommands, or SCM `--service` launch.
- Responsibilities: Dispatch mode and start the shared server.

**Windows SCM Adapter:**
- Location: `src/win_service.rs`.
- Triggers: `install`, `uninstall`, `start`, `stop`, `status`, `--service`.
- Responsibilities: Service registration, state reporting, stop signaling, and log redirection.

**Browser/Node Client:**
- Location: `api/skf_api.js`.
- Triggers: Browser demo or Node tests.
- Responsibilities: WebSocket lifecycle, request ID correlation, and high-level Promise methods.

**Release Build:**
- Location: `packaging/windows/build.ps1` and `.github/workflows/release-windows.yml`.
- Triggers: Manual PowerShell execution, workflow dispatch, or pushed version tag.

## Error Handling

**Strategy:**
- Startup and service-management failures use `anyhow::Result` with contextual messages.
- Request handlers validate parameters manually and return `RpcResponse::err(code, message, id)`.
- Native SKF failures preserve hexadecimal return codes in localized messages.
- Missing dynamic symbols are normalized to `SAR_COULDNOTGETFUNCADDR` by `SkfApi`.

**Limitations:**
- Many mutex, JSON, CString, time, and dynamic-library operations still use `unwrap()`.
- Response objects do not implement strict JSON-RPC 2.0 error semantics.
- Error localization is branch-specific and incomplete.

## Cross-Cutting Concerns

**Logging:**
- Mixed `log` macros, `println!`, and unconditional `eprintln!` debug output.
- Service logs are append-only and unstructured.

**Validation:**
- Positional parameters are checked independently in each match arm.
- There are no shared schemas, request-size limits, or typed request DTOs.

**Authentication and Authorization:**
- USB token PIN verification is the only credential check.
- The network API itself has no client authentication or authorization.

**Concurrency:**
- Each WebSocket connection runs in a Tokio task.
- Native calls are synchronous and can block runtime worker threads.
- Shared `RwLock` maps coordinate provider, PIN, and hash state.

**Platform Compatibility:**
- OS-specific provider path selection uses `std::env::consts::OS`.
- Windows GM3000 distribution is constrained to i686 because the vendor DLL is PE32.

---

*Architecture analysis: 2026-09-11*
*Update when dispatch, state ownership, or provider boundaries change*

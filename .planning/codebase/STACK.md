# Technology Stack

**Analysis Date:** 2026-09-11

## Languages

**Primary:**
- Rust 2021 edition — service process, JSON-RPC dispatch, cryptographic helpers, SKF FFI, and Windows SCM integration in `src/`.

**Secondary:**
- JavaScript (CommonJS) — browser/Node WebSocket client and hardware integration tests in `api/skf_api.js` and `tests/`.
- PowerShell and Windows batch — Windows package build, install, portable run, and uninstall in `packaging/windows/`.
- Bash — macOS-oriented integration test orchestration in `tests/run_tests.sh`.
- YAML — provider and platform-specific native library configuration in `config/skf.yaml`.

## Runtime

**Environment:**
- Rust toolchain with Cargo; no `rust-toolchain.toml` pins a compiler version.
- Tokio 1.49 is the asynchronous runtime used by the server.
- Node.js is required only for the JavaScript integration tests, not for the packaged service.
- OpenSSL CLI is required only by the mock `IssueCertificate` flow in `src/main.rs`.

**Package Managers:**
- Cargo with committed `Cargo.lock` for reproducible service builds.
- npm with committed `package-lock.json`; the only direct Node dependency is `ws` 8.19.

## Frameworks

**Core:**
- Tokio 1.49 — asynchronous TCP listener and task runtime.
- tokio-tungstenite/tungstenite 0.21 — WebSocket upgrade and frame handling.
- Warp 0.3.7 — static HTTP server for the API demo.
- libloading 0.8.9 — runtime loading of vendor SKF DLL/SO/dylib symbols.

**Serialization and Data:**
- serde 1.0, serde_json 1.0, serde_yaml 0.9 — request, response, and configuration parsing.
- base64 0.22 — binary JSON payload encoding.
- x509-parser 0.15 — PKCS#10/X.509 parsing.

**Cryptography:**
- smcrypto 0.3.1 — software SM2/SM4 operations used by mock double-certificate generation.
- rand 0.8.5 — mock session-key generation.
- Vendor SKF implementations — hardware-backed SM2/SM3/SM4/RSA operations through `src/skf/api.rs`.

**Platform Integration:**
- windows-service 0.7 — native Windows Service Control Manager lifecycle.
- windows-sys 0.61 — stdout/stderr redirection for service mode.

**Testing and Quality:**
- Rust built-in test harness via `cargo test`; currently no Rust test functions exist.
- Custom Node integration runner in `tests/test_all_apis.js`; no Jest/Vitest/Mocha dependency.
- rustfmt and Clippy are expected developer tools, but no CI quality gate currently runs them.

## Key Dependencies

**Critical:**
- `libloading` — makes provider support runtime-configurable and creates the unsafe FFI boundary.
- `tokio` + `tokio-tungstenite` — implement the long-running WebSocket gateway.
- `serde_json` + `serde_yaml` — define external request/config formats.
- `smcrypto` + `x509-parser` — implement certificate and national-cryptography support not delegated entirely to the token.
- `windows-service` — enables install/start/stop/status and auto-start behavior on Windows.

**Notable Risk:**
- Cargo reports `serde_yaml 0.9.34+deprecated`; configuration parsing needs a future migration decision.

## Configuration

**Runtime:**
- `config/skf.yaml` selects a default provider, maps VID:PID values, and defines OS-specific native library paths.
- `SKF_CONFIG` overrides the YAML path.
- `SKF_WS_ADDR` overrides the WebSocket bind address; default is `127.0.0.1:9001`.
- `SKF_HTTP_ADDR` overrides the demo HTTP bind address; foreground default is `0.0.0.0:8000`, service default is `127.0.0.1:8000`.
- `RUST_LOG` controls `env_logger` filtering.

**Test:**
- `SKF_PIN`, `SKF_WS_URL`, and `SKF_NO_RESTART` configure `tests/run_tests.sh` and Node integration tests.

**Build:**
- `.cargo/config.toml` defines MinGW cross-linkers for `x86_64-pc-windows-gnu` and `i686-pc-windows-gnu`.
- `packaging/windows/build.ps1` builds the production Windows artifact with `i686-pc-windows-msvc` and static MSVC CRT.
- `.github/workflows/release-windows.yml` builds and publishes the Windows ZIP on version tags.

## Platform Requirements

**Development:**
- Rust/Cargo on macOS, Linux, or Windows.
- Matching vendor middleware and attached USB Key are required for hardware-backed integration tests.
- Node.js and OpenSSL are required for the full existing test flow.

**Production:**
- Current first-class package target is Windows x64 running a 32-bit WoW64 process.
- `skf-service.exe` and `native/GM3000/windows/mtoken_gm3000.dll` must both be PE32/i386 (`Machine=0x014C`).
- The GM3000 kernel/device driver must be installed separately even though the user-mode DLL is bundled.
- macOS GM3000 and Windows FishMan native libraries are tracked, but equivalent release packages are not automated.

---

*Stack analysis: 2026-09-11*
*Update after major dependency or deployment-target changes*

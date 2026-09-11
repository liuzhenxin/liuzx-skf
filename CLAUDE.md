# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

SKF Service 是一个 Rust 服务，通过 WebSocket 和 HTTP 提供统一 API，用于访问多厂商 USB Key 硬件安全模块（HSM）。服务在运行时动态加载厂商专属 SKF 共享库，并通过 WebSocket 以 JSON-RPC 2.0 协议对外暴露。

- WebSocket 服务：`ws://127.0.0.1:9001`
- HTTP API 演示：`http://0.0.0.0:8000`

## 构建与运行

```bash
cargo build              # debug 构建
cargo build --release    # release 构建
cargo run                # 使用默认配置运行
RUST_LOG=debug cargo run # 开启 debug 日志运行
```

## 测试

集成测试为 Node.js 脚本，通过 WebSocket 调用服务。执行测试前服务必须已启动。

```bash
# 完整测试流程（编译 → 部署 → 重启服务 → 测试 → 清理）
./tests/run_tests.sh

# 跳过服务重启（服务已在运行时使用）
SKF_NO_RESTART=1 ./tests/run_tests.sh

# 自定义 PIN（默认：12345678）
SKF_PIN=87654321 ./tests/run_tests.sh

# 自定义 WebSocket 地址（默认：ws://127.0.0.1:9001）
SKF_WS_URL=ws://192.168.1.100:9001 ./tests/run_tests.sh

# USB 插拔事件测试（持续运行，Ctrl+C 退出）
node tests/test_usb_event.js
```

## 代码质量

```bash
cargo fmt       # 格式化代码
cargo clippy    # 代码检查
```

## 交叉编译（Windows 目标）

`.cargo/config.toml` 配置了使用 MinGW 交叉编译到 Windows 的链接器：
- `x86_64-pc-windows-gnu` → `x86_64-w64-mingw32-gcc`
- `i686-pc-windows-gnu` → `i686-w64-mingw32-gcc`

## 架构

### 源码结构

```
src/
├── main.rs          # WebSocket 服务器、JSON-RPC 分发、全部业务逻辑（约 2000 行）
└── skf/
    ├── mod.rs       # 模块重导出
    ├── api.rs       # 动态加载 SKF 库函数的 FFI 封装
    └── types.rs     # C 兼容类型（ULONG、BYTE、HANDLE、ECCPUBLICKEYBLOB 等）
```

### 运行时流程

1. `main.rs` 将 `config/skf.yaml` 加载为 `SkfConfig`
2. 每个 WebSocket 连接创建一个 `SkfContext`（持有已加载的 `SkfApi` 实例和 PIN 缓存）
3. 收到的 JSON-RPC 2.0 消息分发到对应处理函数
4. 处理函数调用 `SkfApi` 方法，进而调用动态加载的厂商 `.dll`/`.so`/`.dylib` 中的 FFI 函数

### 动态库加载

`src/skf/api.rs` 封装了 `libloading::Library`。每个 `SkfApi` 实例持有 `Arc<Library>`，按需解析函数指针。若符号不存在，返回 `SAR_COULDNOTGETFUNCADDR` 而非 panic。

### 厂商配置（`config/skf.yaml`）

```yaml
default: "GM3000"          # 默认提供商
vendor:
  "055c:e618": "GM3000"    # VID:PID → 提供商别名
GM3000:
  windows: "%ProgramFiles(X86)%\\GM3000\\mtoken_gm3000.dll"
  linux:   "native/GM3000/linux/libgm3000.1.0.so"
  macos:   "native/GM3000/macos/x86_64/libgm3000.1.0.dylib"
```

库路径支持 `%VAR%` 环境变量展开。原生库存放于 `native/<厂商名>/<os>/`。

### 关键实现细节

- **DER 编码**（main.rs ~218–382）：手写 X.509 结构的 DER 编码，包括 SubjectDN、SM2 和 RSA 的 SPKI，未使用外部 ASN.1 库。
- **双证书模式**（main.rs ~880–1054）：生成 SM2 密钥对，用随机会话密钥 SM4-ECB 加密私钥，再用签名公钥 SM2 加密会话密钥，最终产生 `ENVELOPEDKEYBLOB`。
- **CSR/证书签发**：通过 shell 调用系统 `openssl` 命令完成 PKCS#10 和 Mock CA 操作。
- **PIN 缓存**：PIN 按设备+应用+容器为键缓存在 `SkfContext` 内存中，服务重启后失效。
- **双语错误消息**：`SetLanguage("CN"|"EN")` 切换当前会话的错误消息语言。

### 支持的算法

| 算法 | 用途 |
|------|------|
| SM2 | 密钥生成、签名、加密（基于 ECC 的国密标准） |
| SM3 | 哈希 |
| SM4 | 对称加密（ECB/CBC） |
| RSA | 密钥生成、签名 |
| SHA1/SHA256 | 哈希 |

### JSON-RPC 2.0 协议

所有参数为位置数组。示例：

```json
{ "jsonrpc": "2.0", "method": "EnumDevice", "params": ["GM3000"], "id": 1 }
```

主要方法：`EnumProvider`、`EnumDevice`、`ConnectDev`、`DisConnectDev`、`EnumApplication`、`EnumContainer`、`CheckPIN`、`FindCertificates`、`SignData`、`Digest`、`CreatePKCS10`、`IssueCertificate`、`ImportCertificate`、`ImportKeyPair`、`GenerateRandom`、`WaitForDevEvent`、`DeleteContainer`、`SetLanguage`。

<!-- GSD:project-start source:PROJECT.md -->
## Project

**LiuZX SKF Service**

LiuZX SKF Service 是一个 Rust 编写的本地 SKF 网关，通过 WebSocket JSON-RPC 风格接口统一访问不同厂商的 USB Key/HSM。它向浏览器和本地应用提供设备管理、PIN 验证、证书、签名、哈希、加解密等能力，并可作为 Windows 原生服务长期运行。

当前正式分发重点是 Windows x64 操作系统上的 GM3000：由于厂商 DLL 为 PE32/i386，服务进程以 i686 运行并通过 WoW64 部署。

**Core Value:** 应用能够通过稳定、安全且与厂商实现解耦的统一接口访问 USB Key 的硬件密码能力。

### Constraints

- **Compatibility**: Windows x64 上的 GM3000 组合必须保持 `skf-service.exe=PE32/i386` 与 `mtoken_gm3000.dll=PE32/i386` — 64 位进程无法加载现有厂商 DLL
- **Hardware**: GM3000 驱动和物理 USB Key 不存在于标准 GitHub Hosted Runner — CI 必须区分 Fake provider 验证和真实硬件 UAT
- **Security**: PIN、私钥、会话密钥和明文业务数据不得进入日志、规划文档或发布元数据 — PKI/密码设备数据均按敏感信息处理
- **FFI**: SKF ABI 名称和 C 布局必须保持规范兼容 — 重构不能随意修改结构大小、对齐、调用约定或符号名
- **API**: v0.2.0 JavaScript 客户端是现有兼容基线 — 协议加固应提供版本策略和迁移路径
- **Operations**: 正式 Windows 包运行时不得要求 Rust、Cargo 或 Node.js — OpenSSL 仅允许作为 Mock 证书接口的可选依赖
- **Privileges**: 服务权限应最小化，但必须先验证厂商驱动在目标账户下可访问 — 不能以降低权限为名破坏硬件可用性
<!-- GSD:project-end -->

<!-- GSD:stack-start source:codebase/STACK.md -->
## Technology Stack

## Languages
- Rust 2021 edition — service process, JSON-RPC dispatch, cryptographic helpers, SKF FFI, and Windows SCM integration in `src/`.
- JavaScript (CommonJS) — browser/Node WebSocket client and hardware integration tests in `api/skf_api.js` and `tests/`.
- PowerShell and Windows batch — Windows package build, install, portable run, and uninstall in `packaging/windows/`.
- Bash — macOS-oriented integration test orchestration in `tests/run_tests.sh`.
- YAML — provider and platform-specific native library configuration in `config/skf.yaml`.
## Runtime
- Rust toolchain with Cargo; no `rust-toolchain.toml` pins a compiler version.
- Tokio 1.49 is the asynchronous runtime used by the server.
- Node.js is required only for the JavaScript integration tests, not for the packaged service.
- OpenSSL CLI is required only by the mock `IssueCertificate` flow in `src/main.rs`.
- Cargo with committed `Cargo.lock` for reproducible service builds.
- npm with committed `package-lock.json`; the only direct Node dependency is `ws` 8.19.
## Frameworks
- Tokio 1.49 — asynchronous TCP listener and task runtime.
- tokio-tungstenite/tungstenite 0.21 — WebSocket upgrade and frame handling.
- Warp 0.3.7 — static HTTP server for the API demo.
- libloading 0.8.9 — runtime loading of vendor SKF DLL/SO/dylib symbols.
- serde 1.0, serde_json 1.0, serde_yaml 0.9 — request, response, and configuration parsing.
- base64 0.22 — binary JSON payload encoding.
- x509-parser 0.15 — PKCS#10/X.509 parsing.
- smcrypto 0.3.1 — software SM2/SM4 operations used by mock double-certificate generation.
- rand 0.8.5 — mock session-key generation.
- Vendor SKF implementations — hardware-backed SM2/SM3/SM4/RSA operations through `src/skf/api.rs`.
- windows-service 0.7 — native Windows Service Control Manager lifecycle.
- windows-sys 0.61 — stdout/stderr redirection for service mode.
- Rust built-in test harness via `cargo test`; currently no Rust test functions exist.
- Custom Node integration runner in `tests/test_all_apis.js`; no Jest/Vitest/Mocha dependency.
- rustfmt and Clippy are expected developer tools, but no CI quality gate currently runs them.
## Key Dependencies
- `libloading` — makes provider support runtime-configurable and creates the unsafe FFI boundary.
- `tokio` + `tokio-tungstenite` — implement the long-running WebSocket gateway.
- `serde_json` + `serde_yaml` — define external request/config formats.
- `smcrypto` + `x509-parser` — implement certificate and national-cryptography support not delegated entirely to the token.
- `windows-service` — enables install/start/stop/status and auto-start behavior on Windows.
- Cargo reports `serde_yaml 0.9.34+deprecated`; configuration parsing needs a future migration decision.
## Configuration
- `config/skf.yaml` selects a default provider, maps VID:PID values, and defines OS-specific native library paths.
- `SKF_CONFIG` overrides the YAML path.
- `SKF_WS_ADDR` overrides the WebSocket bind address; default is `127.0.0.1:9001`.
- `SKF_HTTP_ADDR` overrides the demo HTTP bind address; foreground default is `0.0.0.0:8000`, service default is `127.0.0.1:8000`.
- `RUST_LOG` controls `env_logger` filtering.
- `SKF_PIN`, `SKF_WS_URL`, and `SKF_NO_RESTART` configure `tests/run_tests.sh` and Node integration tests.
- `.cargo/config.toml` defines MinGW cross-linkers for `x86_64-pc-windows-gnu` and `i686-pc-windows-gnu`.
- `packaging/windows/build.ps1` builds the production Windows artifact with `i686-pc-windows-msvc` and static MSVC CRT.
- `.github/workflows/release-windows.yml` builds and publishes the Windows ZIP on version tags.
## Platform Requirements
- Rust/Cargo on macOS, Linux, or Windows.
- Matching vendor middleware and attached USB Key are required for hardware-backed integration tests.
- Node.js and OpenSSL are required for the full existing test flow.
- Current first-class package target is Windows x64 running a 32-bit WoW64 process.
- `skf-service.exe` and `native/GM3000/windows/mtoken_gm3000.dll` must both be PE32/i386 (`Machine=0x014C`).
- The GM3000 kernel/device driver must be installed separately even though the user-mode DLL is bundled.
- macOS GM3000 and Windows FishMan native libraries are tracked, but equivalent release packages are not automated.
<!-- GSD:stack-end -->

<!-- GSD:conventions-start source:CONVENTIONS.md -->
## Conventions

## Naming Patterns
- Rust modules use snake_case: `src/win_service.rs`, `src/skf/api.rs`.
- JavaScript files use snake_case: `api/skf_api.js`, `tests/test_all_apis.js`.
- User-facing landmark documentation uses uppercase names: `README_CN.md`, `RTK.md`, `AGENTS.md`.
- Rust functions use snake_case; async functions have no prefix (`run_server`, `handle_request`).
- SKF wrapper names translate C names to snake_case (`SKF_EnumDev` → `enum_dev`).
- JavaScript public client methods use camelCase (`enumProvider`, `createPKCS10`).
- Rust local variables and fields use snake_case.
- Constants use UPPER_SNAKE_CASE.
- Intentionally unused values use a leading underscore, as in `_provider`.
- Native Rust types use PascalCase (`SkfConfig`, `SkfContext`, `RpcRequest`).
- C ABI aliases and structures retain specification names (`ULONG`, `DEVINFO`, `ECCPUBLICKEYBLOB`) and are protected by allow attributes in `src/skf/`.
- C-layout structures use `#[repr(C)]`; opaque handles are raw pointers.
## Code Style
- Intended formatter is rustfmt (`cargo fmt`).
- Current baseline is not rustfmt-clean: `cargo fmt -- --check` reports 198 diff sections, primarily in `src/main.rs` and `src/skf/api.rs`.
- New Rust modules should be rustfmt-formatted even before a dedicated whole-file normalization change.
- JavaScript uses four-space indentation, semicolons, single-quoted module imports, and template literals for output.
- PowerShell uses four-space indentation and PascalCase command conventions.
- Clippy is the Rust linter (`cargo clippy --all-targets`).
- Current baseline succeeds but emits approximately 72 warnings per binary target, dominated by `req.params.get(0)`, needless borrows, acronym naming, and large function signatures.
- There is no configured JavaScript linter or formatter and no npm scripts.
## Import Organization
- Existing `src/main.rs` does not consistently group or alphabetize imports; use rustfmt-compatible grouping in new modules.
- CommonJS `require()` is the established module format.
## Error Handling
- Return `anyhow::Result<()>` and add context with `anyhow!` or `.context()`.
- Fail startup when config parsing or address binding fails.
- Use early returns when positional parameters are missing or invalid.
- Return `RpcResponse::err` with numeric code, localized message where implemented, and original request ID.
- Return `RpcResponse::ok` with JSON values for success.
- Compare every SKF return value with `SAR_OK`.
- Preserve vendor return codes in hexadecimal error text.
- Missing dynamic symbols return `SAR_COULDNOTGETFUNCADDR` rather than panicking.
- Open device/application/container handles should be closed on every success and error path.
- Do not introduce new `unwrap()` calls on client input, locks, dynamic libraries, or serialization.
- Do not cast unvalidated user-supplied values directly to native handles.
- Prefer typed validation helpers and scoped resource guards.
## Logging
- `log` plus `env_logger` for configurable log levels.
- `println!` and `eprintln!` also appear in existing startup/debug paths.
- Use `log::{debug,info,warn,error}` rather than unconditional debug prints.
- Log lifecycle transitions and actionable provider/native failures with context.
- Never log PINs, private keys, session keys, plaintext cryptographic data, or unwrapped secrets.
- Prefer lengths, algorithm names, provider aliases, and redacted identifiers for diagnostics.
## Comments and Documentation
- Use `///` for public functions and `//!` for module-level behavior, as demonstrated in `src/win_service.rs`.
- Explain FFI safety invariants immediately above unsafe declarations or blocks.
- Document architecture constraints such as x86 process/DLL compatibility near build code.
- Explain protocol quirks, cryptographic formats, cleanup behavior, and platform workarounds.
- Avoid comments that merely restate the next statement.
- Public `SKFClient` methods use JSDoc with parameter and return descriptions.
- Keep server method name and positional parameter order synchronized with `src/main.rs`.
## Function Design
- `handle_request()` is a single very large match with many inline workflows and duplicated open/verify/close sequences.
- FFI wrapper methods are intentionally thin and have signatures matching the C ABI.
- Do not further grow `handle_request()` when a cohesive handler module can be extracted.
- Use typed request structs or validation helpers instead of repeated positional extraction.
- Extract resource ownership into RAII guards where possible.
- Keep cryptographic encoding helpers pure so they can be unit tested without hardware.
## Module Design
- `src/skf/mod.rs` exposes `api` and `types` to the binary crate.
- `SkfApi` is the single wrapper surface over vendor libraries.
- Windows-only code is gated with `#[cfg(windows)]` at module and dependency levels.
- Keep ABI definitions isolated under `src/skf/`.
- Keep platform service management in `src/win_service.rs`.
- Keep package-building behavior under `packaging/`, not in runtime code.
- If introducing a Rust library target, move reusable protocol/domain logic to `src/lib.rs` and keep `main.rs` as composition root.
## Synchronization Requirements
- Server dispatch in `src/main.rs`.
- FFI wrapper/type definitions in `src/skf/api.rs` and `src/skf/types.rs` when applicable.
- Client method in `api/skf_api.js`.
- Integration coverage in `tests/test_all_apis.js`.
- Chinese and English user documentation.
<!-- GSD:conventions-end -->

<!-- GSD:architecture-start source:ARCHITECTURE.md -->
## Architecture

## Pattern Overview
- One Rust executable hosts WebSocket JSON-RPC-style request handling and a static HTTP demo server.
- Vendor support is configured by alias and implemented through lazily loaded SKF shared libraries.
- Most orchestration and business logic resides in one large request dispatcher in `src/main.rs`.
- Cryptographic state and credentials are held in shared process memory; there is no database.
- The same executable supports foreground and Windows SCM service modes.
## Conceptual Layers
- Purpose: Select console or Windows service mode, create Tokio runtime, bind listeners, and handle shutdown.
- Contains: `main()`, `run_server()`, `spawn_ws_client()` in `src/main.rs`; SCM lifecycle in `src/win_service.rs`.
- Depends on: Tokio, Warp, tungstenite, and Windows service APIs.
- Used by: Operators, Windows SCM, WebSocket clients, and browser demo users.
- Purpose: Parse incoming request JSON, maintain per-connection language choice, route method names, and format responses.
- Contains: `RpcRequest`, `RpcResponse`, `Language`, and `handle_request()` in `src/main.rs`.
- Depends on: Shared `SkfContext`, certificate helpers, base64, and all SKF wrappers.
- Used by: Each WebSocket connection task.
- Purpose: Compose device, application, container, certificate, PIN, hashing, signing, and encryption operations.
- Contains: The method match arms inside `handle_request()` in `src/main.rs`.
- Depends on: SKF API wrappers, in-memory PIN/hash state, DER helpers, OpenSSL, and `smcrypto`.
- Used by: JSON-RPC-style method dispatch.
- Purpose: Load YAML configuration, resolve platform library paths, expand `%VAR%`, cache loaded APIs, PINs, and streaming hash handles.
- Contains: `SkfConfig` and `SkfContext` in `src/main.rs`.
- Depends on: serde_yaml, `RwLock<HashMap<...>>`, libloading.
- Used by: All request handlers.
- Purpose: Define the vendor-neutral Rust-facing wrapper over the SKF C ABI.
- Contains: Function pointer aliases and `SkfApi` methods in `src/skf/api.rs`.
- Depends on: C-compatible definitions in `src/skf/types.rs` and vendor shared-library symbols.
- Used by: Application workflow code.
- Purpose: Mirror SKF scalar types, opaque handles, return codes, algorithm IDs, and C-layout structures.
- Contains: `src/skf/types.rs`.
- Depends on: Rust FFI primitives only.
- Used by: `src/skf/api.rs` and `src/main.rs`.
- Purpose: Offer JavaScript consumption, a browser demo, integration scripts, and Windows distribution automation.
- Contains: `api/`, `tests/`, `packaging/windows/`, and `.github/workflows/release-windows.yml`.
## Data Flow
- One `Arc<SkfContext>` is created per process and shared by all WebSocket connections.
- Loaded provider libraries are cached for process lifetime in `SkfContext.apis`.
- Verified PIN strings are cached by provider/device/application key in `SkfContext.pins`.
- Streaming hash handles are cached by string handle key in `SkfContext.hash_handles`.
- Connection language is the only state scoped to an individual WebSocket.
- No state is persisted across process restart except mock CA files in the OS temporary directory.
## Key Abstractions
- Represents process-wide provider configuration and mutable runtime caches.
- Pattern: Shared service context using `Arc` plus `RwLock`.
- Owns an `Arc<Library>` and resolves each SKF symbol on demand.
- Pattern: Dynamic adapter/facade over a C ABI.
- Wraps raw native handles and unsafely declares them `Send + Sync` for async/shared state.
- Pattern: FFI escape hatch whose safety depends entirely on vendor behavior and caller discipline.
- Method strings such as `EnumDevice`, `CreatePKCS10`, and `DigestInit` select inline handler branches.
- Pattern: Large command dispatcher rather than typed route modules.
- Logical name such as `GM3000` maps to platform-specific shared-library paths.
- Pattern: Configuration-driven native plugin selection.
## Entry Points
- Location: `src/main.rs`.
- Triggers: Console execution, service management subcommands, or SCM `--service` launch.
- Responsibilities: Dispatch mode and start the shared server.
- Location: `src/win_service.rs`.
- Triggers: `install`, `uninstall`, `start`, `stop`, `status`, `--service`.
- Responsibilities: Service registration, state reporting, stop signaling, and log redirection.
- Location: `api/skf_api.js`.
- Triggers: Browser demo or Node tests.
- Responsibilities: WebSocket lifecycle, request ID correlation, and high-level Promise methods.
- Location: `packaging/windows/build.ps1` and `.github/workflows/release-windows.yml`.
- Triggers: Manual PowerShell execution, workflow dispatch, or pushed version tag.
## Error Handling
- Startup and service-management failures use `anyhow::Result` with contextual messages.
- Request handlers validate parameters manually and return `RpcResponse::err(code, message, id)`.
- Native SKF failures preserve hexadecimal return codes in localized messages.
- Missing dynamic symbols are normalized to `SAR_COULDNOTGETFUNCADDR` by `SkfApi`.
- Many mutex, JSON, CString, time, and dynamic-library operations still use `unwrap()`.
- Response objects do not implement strict JSON-RPC 2.0 error semantics.
- Error localization is branch-specific and incomplete.
## Cross-Cutting Concerns
- Mixed `log` macros, `println!`, and unconditional `eprintln!` debug output.
- Service logs are append-only and unstructured.
- Positional parameters are checked independently in each match arm.
- There are no shared schemas, request-size limits, or typed request DTOs.
- USB token PIN verification is the only credential check.
- The network API itself has no client authentication or authorization.
- Each WebSocket connection runs in a Tokio task.
- Native calls are synchronous and can block runtime worker threads.
- Shared `RwLock` maps coordinate provider, PIN, and hash state.
- OS-specific provider path selection uses `std::env::consts::OS`.
- Windows GM3000 distribution is constrained to i686 because the vendor DLL is PE32.
<!-- GSD:architecture-end -->

<!-- GSD:skills-start source:skills/ -->
## Project Skills

No project skills found. Add skills to any of: `.claude/skills/`, `.agents/skills/`, `.cursor/skills/`, or `.github/skills/` with a `SKILL.md` index file.
<!-- GSD:skills-end -->

<!-- GSD:workflow-start source:GSD defaults -->
## GSD Workflow Enforcement

Before using Edit, Write, or other file-changing tools, start work through a GSD command so planning artifacts and execution context stay in sync.

Use these entry points:
- `/gsd-quick` for small fixes, doc updates, and ad-hoc tasks
- `/gsd-debug` for investigation and bug fixing
- `/gsd-execute-phase` for planned phase work

Do not make direct repo edits outside a GSD workflow unless the user explicitly asks to bypass it.
<!-- GSD:workflow-end -->

<!-- GSD:profile-start -->
## Developer Profile

> Profile not yet configured. Run `/gsd-profile-user` to generate your developer profile.
> This section is managed by `generate-claude-profile` -- do not edit manually.
<!-- GSD:profile-end -->

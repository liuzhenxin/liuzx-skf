# Phase 1: Structural Foundation and Test Seam - Research

**Researched:** 2026-09-11
**Domain:** Rust binary→library crate extraction, FFI provider seam design, golden-fixture contract freezing
**Confidence:** HIGH (all findings derived from direct source inspection of this repository; no external version claims made)

<user_constraints>
## User Constraints (from CONTEXT.md)

**CRITICAL:** These are locked decisions from `01-CONTEXT.md`. They MUST be honored by the planner.

### Locked Decisions
- **D-01:** 全量领域操作 trait；`NativeSkfProvider` 内部包装现有 `SkfApi`；trait 方法按业务动作定义，不逐符号镜像 C ABI。
- **D-02:** trait 句柄类型为 provider 自定义 newtype（如 `DeviceHandle(u64)`），原生指针封在 provider 内部，不越过 trait 边界。
- **D-03:** provider 层定义 typed error enum，内部保留 SKF 十六进制返回码。
- **D-04:** 明确排除逐符号镜像 trait。
- **D-05:** 只移动：`src/main.rs` 353–540 行纯编码函数 → `crypto/`；配置 → `config/`；新增 `provider/`；`run_server` 接受可注入 provider。
- **D-06:** `handle_request` 与 37 个方法分支保留在 `main.rs`，本阶段不迁移。
- **D-07:** 协议层抽取（`RpcRequest`/`RpcResponse`/`Language`/参数解析 → `protocol/`）推迟到阶段 2。
- **D-08:** 领域处理函数抽取（37 分支 → `domain/`）推迟到阶段 2。
- **D-09:** 契约冻结采用重构前录制 + 端到端重放。硬性顺序：**先录制、先提交 fixture，再开始重构**。
- **D-10:** fixture 绝不允许由阶段 1 新写的 fake provider 生成（反循环论证）。
- **D-11:** 需要硬件的方法，无设备时的确定性错误响应同样构成有效契约，按实际响应录制，不跳过。
- **D-12:** fixture 存于 `tests/fixtures/v0.2.0/<Method>.json`，含 `request`/`response`/`provenance`。
- **D-13:** fixture 以端到端方式重放：可注入 provider + 真实服务器，临时端口，比对完整响应 JSON。
- **D-14:** fake 只实现测试消费的方法子集，failure injection API 从第一天完备。
- **D-15:** 失败注入按调用配置：`fail_next(Operation, SkfError)`，可组合。
- **D-16:** fake 记录调用序列，供资源释放断言使用。
- **D-17:** 本阶段不追求 37 方法全覆盖的 fake。
- **D-18:** `SKF_CONFIG`、`SKF_WS_ADDR`、`SKF_HTTP_ADDR`、`RUST_LOG` 继续生效。
- **D-19:** `config/skf.yaml` 与 `packaging/windows/skf-windows-x86.yaml` 继续可解析，键名与语义不变。
- **D-20:** FFI 符号名 / C 布局 / 调用约定规范兼容；`i686-pc-windows-gnu` 检查与 `i686-pc-windows-msvc` 打包继续通过。
- **D-21:** 不引入重量级新依赖；阶段 1 不新增运行时必需的第三方 crate。

### the agent's Discretion
- 新模块的确切文件名与目录嵌套层级
- trait 方法的具体命名与签名细节
- fixture 测试的辅助函数与测试组织方式
- `provenance` 字段的具体编码格式
- fake provider 的内部数据结构与调用记录格式
- 纯函数提取时是否顺带补充文档注释

### Deferred Ideas (OUT OF SCOPE)
- 协议层抽取 → 阶段 2
- 领域层抽取（37 分支）→ 阶段 2
- 会话注册表与授权隔离 → 阶段 2
- 不透明资源标识与确定性释放 → 阶段 2
- 传输限制、超时、阻塞 FFI 隔离、按设备串行化 → 阶段 3
- 格式化与 Clippy 归一化 → 阶段 4
- `tempfile` / `proptest` 引入 → 视阶段 1 实际需要
- 稳定错误码对外暴露与协议版本协商 → 阶段 6
- `IssueCertificate` 双证书正确性修复 → 不在任何阶段
- `serde_yaml` 替换 → 独立里程碑

</user_constraints>

<architectural_responsibility_map>
## Architectural Responsibility Map

本服务是**单进程单层应用**（本地守护进程），但阶段 1 内部存在明确的分层归属：

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| 纯编码逻辑（DER / SubjectDN / SPKI / base64 / hex） | Library crate — `crypto/` | — | 无 I/O、无 FFI、无状态，是唯一可零依赖单测的层 |
| 配置解析与 `%VAR%` 展开 | Library crate — `config/` | — | 纯字符串/结构处理，输入是文件内容而非文件句柄 |
| SKF 调用边界 | Library crate — `provider/` | — | 唯一 `unsafe` 汇聚点，也是唯一可被 fake 替换的接缝 |
| 进程装配与启动编排 | Binary crate — `main.rs` | Library `server/` | 组合根保留在二进制，可注入依赖通过参数传入 |
| 协议解析与领域分派 | Binary crate — `main.rs`（本阶段**不迁移**） | — | 阶段 2 迁移；本阶段冻结其可观测行为 |
| 契约回归防线 | Test tier — `tests/` + `tests/fixtures/` | — | 端到端重放，不依赖硬件 |

</architectural_responsibility_map>

<research_summary>
## Summary

本阶段的技术难点不在"如何拆 crate"——那是标准 Rust 操作——而在**如何在不拥有硬件的 CI 环境中证明重构没有改变已发布行为**。研究围绕这一目标展开，并产生了四项会直接改变计划写法的发现。

**发现一：`run_server` 当前无法在临时端口上被测试。** 它接收 `SKF_WS_ADDR` 并 `TcpListener::bind`，但只把**请求的**地址打印出来，从不暴露 `listener.local_addr()`。若传入 `127.0.0.1:0`，调用方无法得知实际端口，测试也就无法连接。端到端 fixture 重放（D-13）要求先解决这一点：要么由调用方传入已绑定的 `TcpListener`，要么让 `run_server` 回传实际绑定地址。这是阶段 1 的一个硬性前置改动。

**发现二：多数响应的部分字段本质上是易变的，精确 JSON 比对不可能全局成立。** 实测确认的易变来源包括：`ConnectDev` 返回原生指针的十进制字符串（`format!("{}", h_dev as usize)`）；`GenerateRandom` 返回随机字节；`CheckPIN` 失败消息内嵌剩余重试次数；`CreatePKCS10` 返回含时间戳的容器名与全新密钥的 CSR；`IssueCertificate` 返回 OpenSSL 生成的证书；`FindCertificates` / `EnumDevice` / `GetDevInfo` 返回设备相关内容。因此 fixture 必须在**录制当时**确定并冻结"易变字段清单"，而不是事后为了让测试通过再调整。这直接影响 `provenance` 的设计（D-12 赋予的自由裁量范围）。

**发现三：库加载在 37 个分支中不一致，且存在按次加载后释放的模式。** 实测统计：`handle_request` 内 `ctx.get_api(` 出现 25 次，`ctx.get_lib_path(` 出现 7 次，而**直接 `Library::new(` 出现 10 次**（`LockDev`、`UnlockDev`、`Transmit`、`RSAVerify`、`GenECCKeyPair`、`GenRSAKeyPair`、`DigestInit`、`DigestUpdate`、`DigestFinal`、`CloseHash`）。这些分支在 arm 结束时 drop 掉 `SkfApi`（内含 `Arc<Library>`），而它们产出的原生句柄会在后续请求中继续被使用（例如 `DigestInit` 存下 `h_hash`，`DigestUpdate` 又新建一个 `Library` 来调用它）。这既是 D-01「provider 拥有库生命周期」的直接依据，也是阶段 2 RES-03/RES-04 的真实风险来源。**阶段 1 不修此缺陷**（属行为变更），但 provider 边界必须设计成修好之后无需再改签名。

**发现四：句柄与清理的分布极不均匀。** 37 个分支中 19 个的 `connect_dev`/`open_application`/`open_container` 出现次数多于对应的 close 调用次数；单分支内 `return RpcResponse::err` 最多达 25 处（`SignData`）。这不是"错误路径必然泄漏"的证明（多分支共用多处 close），但足以说明清理逻辑靠人工逐路径维护，与阶段 2 引入 RAII 守卫的必要性一致。provider trait 应让"打开即可获得 Drop 守卫"成为默认写法。

其余结论：不需要任何新依赖；`[lib]` 拆分不会改变产物名；`crypto/` 的 15 个纯函数是零风险的首批提取目标。

**Primary recommendation:** 计划必须按 `录制 → 提交 fixture → 建立 provider 边界与库测试 → 拆分 lib → 验证` 的顺序组织，其中端口暴露与易变字段规范是录制任务的前置条件。
</research_summary>

<standard_stack>
## Standard Stack

### Core（全部为既有依赖，阶段 1 不新增）
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| （无新增） | — | — | D-21 明确不引入重量级新依赖 |
| cargo test harness | 内置 | Rust 单测与集成测试 | 无需引入测试框架；`Cargo.toml` 目前无 `[lib]`，这是零测试的根因 |
| tokio | 1.49（既有） | 测试内启动真实服务器 | 已启用 `full` feature，含 `#[tokio::test]` 所需宏 |
| serde_json | 1（既有） | fixture 序列化与比对 | 已用于 RPC 编解码，比对时无需新工具 |

### Supporting（仅在确有需要时引入，需在计划中显式论证）
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tempfile | dev-dependency | 测试用临时目录与自动清理 | 仅当出现需要真实文件系统状态且无法用 `std::env::temp_dir()` + 手动清理覆盖的测试 |
| （无其他） | — | — | — |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `cargo test` 内置断言 | `assert_json_diff` 等比对库 | 会引入新依赖（违反 D-21 精神）；手写递归比对足以覆盖 37 个方法的响应形状 |
| 自定义 fixture 重放 | `insta` 快照测试 | `insta` 的"接受新快照"一键更新机制**正是** D-10 要防的风险——它让"行为变了"和"fixture 跟着变了"难以区分 |
| `mockall` 生成 fake | 手写 fake | `mockall` 对 trait 对象与 async 支持受限，且生成代码的可读性差；手写 fake 记录调用序列（D-16）更直接 |

**Installation:**
```bash
# 阶段 1 预期不需要执行任何 cargo add
# 若计划决定引入 tempfile，则为 dev-dependency：
cargo add --dev tempfile
```
</standard_stack>

<architecture_patterns>
## Architecture Patterns

### System Architecture Diagram

```
                        ┌─────────────────────────────────────────┐
                        │  Binary crate: src/main.rs              │
                        │  (composition root)                     │
                        │                                         │
  CLI args ────────────►│  main()                                 │
  (install/uninstall/   │    ├─ Windows SCM subcommands           │
   start/stop/status/   │    ├─ env_logger::init()                │
   --service)           │    └─ build tokio runtime               │
                        │         │                               │
                        │         └──► run_server(args)           │
                        │                 │                       │
                        │                 ├─ config::load(path)  │
                        │                 ├─ provider::build()   │
                        │                 └─ server::serve()     │
                        └─────────────────┬───────────────────────┘
                                          │
              ┌───────────────────────────┼────────────────────────────┐
              ▼                           ▼                            ▼
   ┌─────────────────────┐   ┌────────────────────────┐   ┌─────────────────────┐
   │ Library: config/    │   │ Library: server/       │   │ Library: provider/  │
   │                     │   │                        │   │                     │
   │ load yaml           │   │ TcpListener bind       │   │ trait SkfProvider   │
   │ expand %VAR%        │   │ expose local_addr()    │   │   ├ Native (SkfApi) │
   │ validate providers  │   │ accept loop            │   │   └ Fake (tests)    │
   └─────────┬───────────┘   │ ws upgrade             │   └──────┬──────────────┘
             │               │ dispatch ──────────────┼──────────┘
             │               └────────────────────────┘          │
             │                                                  │ owns Arc<Library>
             │                                                  ▼
             │                                     ┌──────────────────────────┐
             │                                     │ src/skf/api.rs (SkfApi)  │
             │                                     │ 50 thin symbol wrappers  │
             │                                     └────────────┬─────────────┘
             │                                                  │ unsafe FFI
             │                                                  ▼
             │                                     ┌──────────────────────────┐
             │                                     │ Vendor DLL (GM3000)       │
             │                                     └──────────────────────────┘
             │
             ▼
   ┌─────────────────────┐        ┌────────────────────────────────────────┐
   │ Library: crypto/    │        │ Test tier: tests/                      │
   │ pure DER/SPKI/base64│◄───────│  ├ unit tests (crypto, config)         │
   └─────────────────────┘        │  ├ fixture replay (fake provider +     │
                                  │  │   real server on 127.0.0.1:0)       │
                                  │  └ tests/fixtures/v0.2.0/*.json        │
                                  └────────────────────────────────────────┘
```

### Recommended Project Structure

```
src/
├── main.rs                  # composition root only (thin binary)
├── lib.rs                   # pub mod crypto; pub mod config; pub mod provider; pub mod server;
├── crypto/
│   └── mod.rs               # 15 pure fns moved verbatim from main.rs 353–540
├── config/
│   └── mod.rs               # SkfConfig, load(), expand_env_vars(), validate(), ResolvedProvider
├── provider/
│   ├── mod.rs               # trait SkfProvider, ProviderError, handle newtypes, Operation enum
│   ├── native.rs            # NativeSkfProvider wrapping SkfApi
│   └── fake.rs              # FakeSkfProvider (test-only: #[cfg(test)] or feature-gated)
├── server/
│   └── mod.rs               # run_server + injectable provider + bound-address exposure
├── win_service.rs           # unchanged, moved into library crate (cfg(windows))
└── skf/                     # unchanged (api.rs, types.rs, mod.rs)

tests/
├── fixtures/
│   └── v0.2.0/
│       ├── SetLanguage.json
│       ├── EnumProvider.json
│       └── ... (37 files)
├── contract_fixtures.rs     # end-to-end fixture replay
├── crypto.rs                # pure-logic unit tests (may live in src/crypto as #[cfg(test)])
└── config.rs                # config validation tests

tools/                        # optional: recording helper (may also live under tests/)
└── record_fixtures.sh / .ps1
```

### Pattern 1: Library + Thin Binary Split

**What:** Add `src/lib.rs` and move logic into it; `src/main.rs` keeps only argument dispatch and composition.
**When to use:** Immediately, as the first structural change — it is the precondition for every other test in the milestone.
**Key mechanic (verified):** With package name `skf-service`, adding `src/lib.rs` produces lib target `skf_service` while the binary from `src/main.rs` keeps the name `skf-service`, so `target/<target>/release/skf-service.exe` is **unchanged** and `packaging/windows/build.ps1` continues to work without modification.

```rust
// src/lib.rs
pub mod config;
pub mod crypto;
pub mod provider;
pub mod server;
pub mod skf;

#[cfg(windows)]
pub mod win_service;
```

```rust
// src/main.rs  — stays thin
fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    { /* unchanged SCM subcommand dispatch */ }

    env_logger::init();
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(skf_service::server::run_server(
        skf_service::server::ServerOptions::from_env(),
        |config| skf_service::provider::native::build(config), // injectable
    ))
}
```

### Pattern 2: Domain-Operation Provider Trait with Opaque Handles

**What:** A trait whose methods express business actions; handles are provider-defined newtypes so native pointers never cross the boundary.
**When to use:** Now (defines the shape), fully exploited in Phase 2.
**Derived from actual usage:** The 37 branches collectively use 40 distinct `SkfApi` methods. The trait collapses these into a smaller set of *operations* grouped by resource lifecycle:

| Operation group | Underlying SkfApi calls observed | Trait shape |
|-----------------|----------------------------------|-------------|
| Library/device enumeration | `enum_dev`, `get_dev_state`, `cancel_wait_for_dev_event`, `wait_for_dev_event` | `enum_devices()`, `device_state(name)`, `wait_for_event(..)` |
| Device resource | `connect_dev`, `dis_connect_dev`, `lock_dev`, `unlock_dev`, `transmit`, `gen_random` | `open_device(name) -> DeviceGuard`, guard methods for lock/unlock/transmit/random |
| Application resource | `open_application`, `close_application`, `enum_application`, `verify_pin` | `open_application(dev, name) -> AppGuard`, `verify_pin(&app, pin) -> PinOutcome` |
| Container resource | `open_container`, `close_container`, `enum_container`, `create_container`, `delete_container`, `get_container_type`, `export_certificate`, `import_certificate` | `open_container(app, name) -> ContainerGuard`, container methods on guard |
| Signing | `ecc_sign_data`, `rsa_sign_data`, `ecc_verify`, `rsa_verify` | `sign_ecc(&container, digest)`, `sign_rsa(&container, data)`, verify variants |
| Digest (stateful) | `digest_init`, `digest_update`, `digest_final`, `digest`, `close_hash` | `digest_begin(dev, alg) -> DigestGuard`, guard methods |
| Key import/generation | `gen_ecc_key_pair`, `gen_rsa_key_pair`, `import_ecc_key_pair`, `import_rsa_key_pair` | `gen_keypair(..)`, `import_keypair(..)` |
| Symmetric | `set_symm_key`, `encrypt_data`, `decrypt_data` | `set_symm_key`, `encrypt`, `decrypt` |
| Device info/label | `get_dev_info`, `set_label` | `device_info(&dev)`, `set_label(&dev, label)` |

```rust
// src/provider/mod.rs
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceHandle(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AppHandle(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContainerHandle(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DigestHandle(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    /// Native SKF return code preserved verbatim as the detail.
    Native { code: u32, context: &'static str },
    /// Symbol not present in the loaded library (maps from SAR_COULDNOTGETFUNCADDR).
    SymbolUnavailable { name: &'static str },
    /// Library could not be loaded; includes architecture for actionable errors.
    LibraryLoadFailed { path: String, arch: &'static str, detail: String },
    /// Provider alias has no configuration for the current OS.
    ProviderNotConfigured { alias: String, os: &'static str },
}

pub type ProviderResult<T> = Result<T, ProviderError>;

/// Resources returned by the provider own their native handle and MUST release it on Drop.
/// Phase 2 replaces the `u64` counters with real per-session opaque IDs; the *shape*
/// of these guards must not need to change then.
pub trait DeviceGuard {
    fn handle(&self) -> DeviceHandle;
    fn state(&self) -> ProviderResult<u32>;
    fn lock(&self, timeout: Duration) -> ProviderResult<()>;
    fn unlock(&self) -> ProviderResult<()>;
    fn transmit(&self, command: &[u8]) -> ProviderResult<Vec<u8>>;
    fn random(&self, len: usize) -> ProviderResult<Vec<u8>>;
}

pub trait SkfProvider: Send + Sync {
    fn alias(&self) -> &str;

    fn enum_devices(&self, present_only: bool) -> ProviderResult<Vec<String>>;

    fn open_device(&self, name: &str) -> ProviderResult<Box<dyn DeviceGuard>>;

    // ... open_application / open_container / digest_begin follow the same guard pattern
}
```

**Critical design note:** the trait must **not** expose `*mut c_void`, `HANDLE`, or `SendHandle`. Guards are created by the provider and release their native resource in `Drop`, which is what makes RES-03/RES-04 achievable in Phase 2 without changing these signatures.

### Pattern 3: Failure-Injecting Fake with Call Recording

```rust
// src/provider/fake.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    EnumDevices, OpenDevice, OpenApplication, VerifyPin, OpenContainer,
    SignEcc, SignRsa, DigestBegin, DigestUpdate, DigestFinal, CloseHash, // extend as tests demand
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkfError {
    PinIncorrect, PinLocked, ContainerMissing, SymbolUnavailable,
    DeviceRemoved, Blocking(Duration), Custom(u32),
}

pub struct FakeSkfProvider {
    /// Composable per-call injection: failures are consumed in order per operation.
    failures: Mutex<HashMap<Operation, VecDeque<SkfError>>>,
    /// Recorded call sequence for release/order assertions (D-16).
    calls: Mutex<Vec<Operation>>,
    /// Deterministic handle counter so fixtures never depend on real pointers.
    next_handle: AtomicU64,
}

impl FakeSkfProvider {
    pub fn fail_next(&self, op: Operation, err: SkfError) { /* push_back */ }
    pub fn calls(&self) -> Vec<Operation> { /* clone */ }
}

// Emitted in Drop so Phase 2 can assert release on every path:
// calls.push(Operation::OpenDevice) on open; calls.push(Operation::CloseDevice) on drop.
```

### Pattern 4: Fixture Recorded Pre-Refactor with Frozen Normalization Spec

**What:** Each fixture file pins the response the *pre-refactor* code produced, plus an explicit list of volatile JSON paths frozen at record time.
**When to use:** Before any production code changes (D-09).

```jsonc
// tests/fixtures/v0.2.0/ConnectDev.json
{
  "method": "ConnectDev",
  "request": { "jsonrpc": "2.0", "method": "ConnectDev", "params": ["<device>"], "id": 1 },
  "response": { "error": 0, "result": "140234567890123", "id": 1 },
  "normalize": {
    "result": "<opaque:usize>"
  },
  "provenance": {
    "recorded_from": "git 3ab5f3f (v0.2.0) unmodified working tree",
    "recorded_at": "2026-09-11",
    "requires_hardware": true,
    "observed_with_device": false,
    "volatile_fields_reviewed": true
  }
}
```

**Recording rules (derived from findings two and three):**
1. Volatile paths are declared **at record time** and reviewed in the same commit as the fixture. Adjusting a `normalize` entry later to make a failing test pass invalidates the fixture and must be treated as a behavior change.
2. For methods whose entire result is non-deterministic (`GenerateRandom`, `CreatePKCS10`, `IssueCertificate`), the fixture records the **shape** and declared invariants (e.g. `result` is a base64 string of declared length; container name matches `CSR\d{8}`), not exact bytes.
3. Volatile fields confirmed by inspection that must be normalized: pointer-as-string (`ConnectDev`), random bytes (`GenerateRandom`), retry counters embedded in error messages (`CheckPIN` failure text), timestamps in container names (`CreatePKCS10`), OpenSSL-generated material (`IssueCertificate`), device-dependent values (`EnumDevice`, `GetDevInfo`, `FindCertificates`).
4. Deterministic values that must **not** be normalized: `EnumProvider` output (config-derived), `SetLanguage` result, `DigestInit` handle string (`hash_{id}` — derived from the JSON-RPC id, hence deterministic), all error codes, and all `error`/`message` for non-retry-bearing failures.

### Pattern 5: Exposing the Bound Address for Ephemeral-Port Testing

**What:** `run_server` currently prints the requested address but never the resolved one, so `127.0.0.1:0` is unusable by tests.

```rust
// src/server/mod.rs
pub struct ServerOptions {
    pub config_path: String,
    pub ws_addr: String,
    pub http_addr: Option<String>,
}

pub struct BoundServer {
    pub ws_addr: std::net::SocketAddr,   // listener.local_addr() — resolved, not requested
    pub http_addr: Option<std::net::SocketAddr>,
}

/// Binds first, reports the resolved addresses, then serves.
pub async fn bind(opts: &ServerOptions) -> anyhow::Result<(BoundServer, TcpListener)> { /* ... */ }
```

Tests then read `bound.ws_addr.port()` and connect. **This is a required change, not optional** — without it D-13 cannot be implemented.

### Anti-Patterns to Avoid
- **Moving `handle_request` while freezing fixtures:** contradicts D-06 and destroys the ability to attribute behavior changes.
- **Generating fixtures from the fake provider:** the tautology identified in D-10. Recording must run the unmodified pre-refactor binary with the real provider.
- **Normalizing failures away:** adding a `normalize` path for a field only after a test fails turns the regression suite into a rubber stamp.
- **Per-call `Library::new` in new provider code:** the existing 10 occurrences are a known latent defect (finding three); the new `NativeSkfProvider` must own one `Arc<Library>` for its lifetime. Fixing the *branches* is Phase 2 work, but the new provider must not replicate the pattern.
- **Exposing `*mut c_void` through the trait for "flexibility":** violates D-02 and re-opens RES-01.
- **Making the fake always succeed:** research pitfall 12; the fake must be able to fail from its first commit.
- **Adding `#[cfg(test)]`-only modules to the library without a test target:** `cargo test` must report a non-zero count (FOUND-01), so tests need a real target — either `tests/*.rs` or in-module `#[cfg(test)] mod tests` inside the library, both of which now work once `src/lib.rs` exists.

</architecture_patterns>

<dont_hand_roll>
## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Rust library/binary split | A build script or separate crate | Cargo's built-in `src/lib.rs` + `src/main.rs` dual target | Native support; keeps binary name and Windows packaging untouched |
| JSON comparison in tests | A bespoke diff utility from scratch with no normalization model | `serde_json::Value` recursive comparison plus a **declared** path-normalization list | The hard part is not diffing, it is deciding *what is volatile*; that decision must be explicit data, not code |
| Test framework | A custom harness | `cargo test` with `#[test]` / `#[tokio::test]` | Already available; `tokio`'s `full` feature is enabled |
| Resource cleanup bookkeeping | Manual open/close pairing per branch (the current pattern, with up to 25 early returns per arm) | Drop-based guards inside the provider | Eliminates the 19 arms with imbalanced open/close counts at the source |
| Fixture updating UX | Snapshot auto-accept tooling (`insta`-style) | Explicit, reviewed fixture diffs | Auto-accept collapses the distinction between a behavior change and a fixture change, defeating D-10 |
| Config path expansion | A new templating crate | The existing `expand_env_vars` moved verbatim to `config/` | It already handles unclosed `%` and unknown vars; rewriting risks behavior drift |
| Fake provider generation | `mockall` / procedural mock crates | Hand-written fake with call recording | Need call-sequence assertions and composable per-call failure injection; generated mocks fight both |

**Key insight:** In a behavior-preserving refactor, the hard engineering is not the code motion — it is constructing a regression oracle that is genuinely independent of the code being changed. Every "don't hand-roll" decision above exists to keep the oracle honest: fixtures recorded from the old binary, volatility frozen at record time, and no auto-accept path.
</dont_hand_roll>

<common_pitfalls>
## Common Pitfalls

### Pitfall 1: Fixture suite that cannot fail
**What goes wrong:** The suite passes before and after any change because every value was normalized or the fixtures were produced by the new fake.
**Why it happens:** When a fixture mismatches, the fastest fix is to widen normalization — which silently deletes the assertion.
**How to avoid:** Normalization spec is frozen in the same commit as the recorded response; any later change to `normalize` must be accompanied by an explicit note that the contract changed. Add a self-test proving the oracle can fail (e.g., a deliberately mutated response must be rejected).
**Warning signs:** `normalize` blocks containing broad wildcards; no negative test proving rejection; fixture diffs appearing during refactor commits with no comment.

### Pitfall 2: Recording fixtures requires a device, so the work stalls
**What goes wrong:** Recording is interpreted as "needs a USB Key", so it is deferred, and the refactor starts unprotected.
**Why it happens:** The test suite is hardware-oriented, and most methods touch the device.
**How to avoid:** Exploit D-11 — in the absence of a device, the deterministic *error* responses are valid contract. Record what the machine can produce locally (provider enumeration from config, language switching, param validation errors, library-load errors), and record device-requiring methods as their device-absent error contract. Mark `requires_hardware: true, observed_with_device: false` in provenance so a later hardware run can enrich them.
**Warning signs:** Plan tasks blocked on hardware; no fixtures committed before refactor commits begin.

### Pitfall 3: `run_server` refactor accidentally changes bind behavior
**What:** While making the port observable, the default binding behavior changes (e.g. service mode stops binding loopback, or `0.0.0.0` default is lost).
**Why it happens:** The bind address currently depends on `shutdown.is_some()` to distinguish service vs console mode — an implicit, easy-to-lose signal.
**How to avoid:** Preserve the existing defaults exactly: foreground HTTP `0.0.0.0:8000`, service HTTP `127.0.0.1:8000`, WS `127.0.0.1:9001`, all overridable by the same env vars (D-18). Make the mode explicit in `ServerOptions` rather than inferring it from an `Option`.
**Warning signs:** Env var behavior differences; tests that pass only when run in one mode.

### Pitfall 4: New provider leaks or unloads the library
**What goes wrong:** The new provider mirrors the existing `Library::new` per call, so native handles become invalid across requests and Windows `FreeLibrary` semantics bite.
**Why it happens:** Copying an existing arm's shape into the provider is the path of least resistance.
**How to avoid:** `NativeSkfProvider` holds one `Arc<Library>` for its lifetime; symbol lookup goes through `SkfApi` obtained once. Assert in a test that repeated operations do not re-load the library (e.g. a load counter in the fake, or a unit test on the native provider's construction path).
**Warning signs:** More than one `Library::new` in `src/provider/`; handle values changing between calls.

### Pitfall 5: Behavior drift smuggled into the move
**What goes wrong:** While moving `crypto/` and `config/`, small "obvious improvements" are made (error message wording, edge-case handling, encoding tweak) and the refactor becomes unattributable.
**Why it happens:** Moved code invites cleanup.
**How to avoid:** Move functions **verbatim** in the structural task; any improvement becomes a separate, explicitly-labeled task with its own test. The `config` expansion and DER encoding must produce byte-identical output for the recorded encoder tests.
**Warning signs:** Diffs inside moved functions; changed assertion strings; commit messages mixing "move" with "fix".

### Pitfall 6: Windows build breaks silently
**What goes wrong:** The `[lib]` split renames the linker output, or `win_service` no longer has access to `run_server` after it moved into the library, breaking the i686 package.
**Why it happens:** `win_service.rs` currently calls `crate::run_server(...)`; after the split that path must become `crate::server::run_server(...)` or `skf_service::server::...`.
**How to avoid:** Verify `cargo check --target i686-pc-windows-gnu` after the split (the local cross-check target), and confirm `build.ps1`'s expected path `target\i686-pc-windows-msvc\release\skf-service.exe` is unchanged. Keep `#[cfg(windows)]` at the module boundary so Linux tests still compile.
**Warning signs:** `can't find crate` on the Windows target; changed `dist` names; `skf_service.exe` instead of `skf-service.exe`.

### Pitfall 7: Tests that need the environment they are meant to avoid
**What goes wrong:** A "unit" test reads `config/skf.yaml` from the working directory, or depends on `SKF_*` env vars, making results depend on the machine.
**Why it happens:** The config layer currently resolves relative paths against the process CWD.
**How to avoid:** `config::load` must accept a path (or content) as a parameter rather than reading env/CWD internally; env-var resolution (`SKF_CONFIG`, `SKF_WS_ADDR`, `SKF_HTTP_ADDR`) belongs in the binary's composition root.
**Warning signs:** Tests failing when run from a different directory; tests passing only with specific env vars set; `std::env::var` inside library code.
</common_pitfalls>

<code_examples>
## Code Examples

All examples below are derived from this repository's actual code (marked **DERIVED**) or are standard Rust idioms verifiable by the compiler (marked **STANDARD**). No external documentation was fetched during this research pass.

### Making the bound port observable (DERIVED from current `run_server`)

```rust
// BEFORE (src/main.rs, ~line 293)
let ws_addr = std::env::var("SKF_WS_ADDR").unwrap_or_else(|_| "127.0.0.1:9001".to_string());
let listener = TcpListener::bind(&ws_addr).await
    .map_err(|e| anyhow::anyhow!("failed to bind WebSocket listener on {}: {}", ws_addr, e))?;
println!("SKF Service listening on ws://{}", ws_addr);   // <-- prints REQUESTED addr

// AFTER (src/server/mod.rs) — resolved address is returned to the caller
let listener = TcpListener::bind(&ws_addr).await
    .map_err(|e| anyhow::anyhow!("failed to bind WebSocket listener on {}: {}", ws_addr, e))?;
let bound_ws = listener.local_addr()?;                    // <-- RESOLVED addr
log::info!("SKF Service listening on ws://{}", bound_ws);
```

### Preserving the mode-dependent HTTP default (DERIVED)

```rust
// Current behavior that MUST be preserved exactly (D-18):
//   console mode  -> 0.0.0.0:8000
//   service mode  -> 127.0.0.1:8000
//   SKF_HTTP_ADDR overrides both
//
// Replace the implicit "is shutdown Some?" signal with an explicit mode:
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RunMode { Console, Service }

let default_http = match mode {
    RunMode::Console => "0.0.0.0:8000",
    RunMode::Service => "127.0.0.1:8000",
};
let http_addr = std::env::var("SKF_HTTP_ADDR").unwrap_or_else(|_| default_http.to_string());
```

### Injecting the provider without changing enforcement (STANDARD pattern, applies D-05)

```rust
// src/server/mod.rs
pub trait ProviderFactory: Send + Sync + 'static {
    fn build(&self, config: &crate::config::SkfConfig) -> std::sync::Arc<dyn crate::provider::SkfProvider>;
}

pub async fn run_server<F>(opts: ServerOptions, factory: F) -> anyhow::Result<()>
where
    F: ProviderFactory,
{
    let config = crate::config::load(&opts.config_path)?;
    let provider = factory.build(&config);
    // ... accept loop unchanged from current implementation
}
```

### Drop-based guard skeleton (STANDARD Rust, satisfies D-02 shape)

```rust
struct NativeDevice {
    api: Arc<SkfApi>,          // keeps the library alive — prevents per-call unload
    raw: DEVHANDLE,
    exposed: DeviceHandle,
}

impl Drop for NativeDevice {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            // SAFETY: `raw` was produced by connect_dev on this same library instance.
            let _ = self.api.dis_connect_dev(self.raw);
        }
    }
}
```

### Fixture replay harness skeleton (STANDARD pattern, implements D-13)

```rust
// tests/contract_fixtures.rs
#[tokio::test]
async fn contract_v0_2_0_fixtures() {
    let (bound, listener) = skf_service::server::bind(&test_options("127.0.0.1:0"))
        .await
        .expect("bind");
    let addr = bound.ws_addr;
    tokio::spawn(skf_service::server::serve(listener, /* fake provider */));

    for case in load_fixtures("tests/fixtures/v0.2.0") {
        let actual = send_rpc(addr, &case.request).await;
        let normalized = apply_normalize(actual, &case.normalize);
        assert_eq!(normalized, case.response, "contract drift in {}", case.method);
    }
}
```

### Oracle self-test — proves the suite can actually fail (STANDARD pattern, mitigates Pitfall 1)

```rust
#[test]
fn fixture_oracle_rejects_mutated_response() {
    let case = load_fixture("tests/fixtures/v0.2.0/SetLanguage.json");
    let mut mutated = case.response.clone();
    mutated["error"] = serde_json::json!(-99);
    assert_ne!(apply_normalize(mutated, &case.normalize), case.response);
}
```
</code_examples>

<sota_updates>
## State of the Art (2024-2026)

对外部生态的版本时效性**未做联网核实**（本轮研究无检索能力）。以下仅为与本阶段决策相关的取向变化，标注为需要时再验证。

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| 单文件 `main.rs` 承载全部逻辑 | 库 crate + 薄二进制 | 长期共识，非新变化 | 直接解决本仓库 FOUND-01/02 的根因 |
| 手工逐路径 close | Drop 守卫 / 类型状态 | 长期共识 | 本阶段只需提供守卫形态，阶段 2 才迁移调用点 |
| 快照测试自动接受新基线 | 显式、需评审的契约差异 | 工具生态仍在鼓励自动接受，属**反向**趋势 | 本阶段刻意选择手动评审路径以满足 D-10 |

**New tools/patterns to consider:**
- 守卫式资源抽象（RAII）：本阶段仅定义形态，成本低、后续不需改签名。
- 声明式易变字段清单：把"什么算易变"变成可评审的数据而非埋没在代码里。

**Deprecated/outdated:**
- 按次加载并释放动态库：本仓库现存 10 处，属已知缺陷模式，新代码不得复制。

</sota_updates>

<validation_architecture>
## Validation Architecture

Section required by the Nyquist validation gate. Concrete per-task mapping is produced during planning; this section defines the contract the plans must satisfy.

### Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust 内置 `cargo test`（`#[test]` / `#[tokio::test]`，tokio `full` feature 已启用） |
| **Config file** | none — `Cargo.toml` 新增 `[lib]` 目标后即可用；不引入测试框架 |
| **Quick run command** | `cargo test --lib` |
| **Full suite command** | `cargo test` |
| **Estimated runtime** | < 30 s（无硬件、无网络、仅纯逻辑与 fake 驱动的契约重放） |

### Sampling Rate

- **After every task commit:** `cargo test --lib`
- **After every plan wave:** `cargo test` plus `cargo check --target i686-pc-windows-gnu`
- **Before `$gsd-verify-work`:** full suite green；`cargo check --target i686-pc-windows-gnu` green
- **Max feedback latency:** ~30 s

### Wave 0 Requirements

本阶段即"基础设施阶段"，因此 Wave 0 就是它的核心内容：

- [ ] `src/lib.rs` + 模块骨架 —— 使 `cargo test` 存在可运行目标（无此则任何 Rust 测试都无法编译）
- [ ] `tools/record_fixtures`（脚本或测试二进制）—— 用**重构前**代码录制 37 个方法的响应
- [ ] `tests/fixtures/v0.2.0/*.json` —— 录制产物，含冻结的 `normalize` 与 `provenance`
- [ ] `tests/contract_fixtures.rs` —— 端到端重放驱动（fake provider + 真实服务器 + 临时端口）
- [ ] `tests/fixture_oracle.rs` —— 证明 oracle 能失败的负向自检
- [ ] provider 注入点（`run_server` 接受 factory；`bind()` 暴露 `local_addr()`）

### Per-Task Verification Map（框架契约，计划阶段填具体 Task ID）

| Requirement | Automated Command | Verification Type |
|-------------|-------------------|-------------------|
| FOUND-01 | `cargo test 2>&1 \| grep -E "test result: ok\. [1-9]"` | 非零测试数通过 |
| FOUND-02 | `test -f src/lib.rs && grep -q "pub mod" src/lib.rs` | 库目标存在且导出模块 |
| FOUND-03 | `cargo test --test contract_fixtures` | 37 个 fixture 全部重放一致 |
| FOUND-03（防伪） | `cargo test --test fixture_oracle` | 变异响应被拒绝 |
| FOUND-04 | `grep -c "Library::new" src/provider/native.rs` | 恰为 1（单一库实例） |
| FOUND-05 | `cargo test provider::fake` | 五类失败注入各自有断言 |
| FOUND-06 | `cargo test crypto` | 纯函数单测（含 base64/hex 边界、DER 长度编码 128/256 边界） |
| 兼容性（D-18/19/20） | `cargo check --target i686-pc-windows-gnu`；`cargo run` 后手工或脚本校验两个 YAML 仍可解析 | 构建与配置不回归 |

### Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| 真实 GM3000 硬件下的 37 方法契约 | FOUND-03（增强） | 托管环境无驱动与设备 | 在装有驱动的 Windows 机器上重跑录制器，比对既有 fixture 并补全 `observed_with_device: true` 的用例 |
| `i686-pc-windows-msvc` 正式打包 | D-20 | macOS 无法执行 MSVC 工具链 | 在 Windows 构建机运行 `packaging\windows\build.ps1`，确认输出 `dist\skf-service-windows-x64-gm3000-x86\` 且 exe 为 `Machine=0x014C` |

### Validation Sign-Off（计划阶段完成）

- [ ] 每个任务具备自动化验证命令或 Wave 0 依赖
- [ ] 采样连续性：不存在连续 3 个任务缺少自动化验证
- [ ] Wave 0 覆盖全部缺失引用
- [ ] 无 watch-mode 标志
- [ ] 反馈延迟 < 30 s
- [ ] 计划完成后置 `nyquist_compliant: true`

</validation_architecture>

<open_questions>
## Open Questions

1. **无设备环境下 37 个方法各能录到哪些响应？**
   - What we know: 无设备时 `EnumDevice` 返回空列表或错误；库加载失败会给出 `-5 Load Lib Failed`；参数校验错误与 `SetLanguage`/`EnumProvider` 不依赖设备。
   - What's unclear: 具体哪些方法在本机（macOS + `libgm3000.1.0.dylib` 存在但无设备）会返回"设备未发现"而非其他错误。
   - Recommendation: 计划中把"录制并统计可录制方法数"作为显式任务，产出实际计数；不足的方法记录 `observed_with_device: false` 并在 UAT 补全。不要把"全部 37 个都能录到完整成功响应"写成验收标准。

2. **易变字段清单的最终边界**
   - What we know: 已由源码确认六类易变来源（指针、随机、重试次数、时间戳、OpenSSL 材料、设备相关信息）。
   - What's unclear: 是否存在仅在特定配置下才易变的字段（例如不同 provider 别名下的消息文本）。
   - Recommendation: 录制时对每个方法标注易变路径并评审；发现新易变字段必须在 fixture 提交时一并记录，不得事后追加。

3. **`Digest*` 三个方法的 fixture 依赖链**
   - What we know: `DigestInit` 返回的 handle 是 `format!("hash_{}", id)`，由请求 id 决定，因此可确定；`DigestUpdate`/`DigestFinal`/`CloseHash` 依赖该 handle 与内部 `hash_handles` 状态。
   - What's unclear: 重放时是否需要严格保持请求顺序与会话连续性（同一连接内）。
   - Recommendation: 把 `Digest*` 与 `CloseHash` 定义为**有状态序列 fixture**（按序在同一连接内重放），而不是彼此独立的单请求用例。

4. **provider trait 是否需要异步**
   - What we know: 当前所有 SKF 调用都是同步阻塞的；阶段 3 才处理阻塞隔离（TRANS-04）。
   - What's unclear: 现在把 trait 定为同步是否会导致阶段 3 改签名。
   - Recommendation: 阶段 1 保持**同步** trait。阶段 3 的隔离方式是调用侧 `spawn_blocking`，不需要 trait 变 async；若届时需要，守卫形态仍可复用。在计划中显式记录该判断，避免阶段 3 返工。

5. **`win_service.rs` 迁入库 crate 后的调用路径**
   - What we know: 它目前调用 `crate::run_server(Some(stop_rx))`，拆分后路径会变。
   - What's unclear: `#[cfg(windows)]` 模块在库 crate 中的可见性与 `main.rs` 的引用方式。
   - Recommendation: `win_service` 作为库的 `#[cfg(windows)]` 公共模块，`main.rs` 通过 `skf_service::win_service::...` 引用；windows-service/windows-sys 依赖继续留在 `[target.'cfg(windows)'.dependencies]`。
</open_questions>

<sources>
## Sources

### Primary (HIGH confidence) — 本仓库直接证据
- `src/main.rs` — 逐分支静态分析：37 个方法、40 个不同 `SkfApi` 调用、`Library::new` 10 处、`ctx.get_api` 25 处、`ctx.get_lib_path` 7 处、`return RpcResponse::err` 248 处、19 个分支 open/close 计数不平衡
- `src/main.rs` 353–540 行 — 15 个纯编码函数清单（`hex_to_bytes`、`der_*` ×11、`build_subject_dn`、`build_sm2_spki`、`build_rsa_spki`）
- `src/main.rs` 675–700 行 — `ConnectDev` 返回 `format!("{}", h_dev as usize)`，确认指针值泄漏到响应
- `src/main.rs` 1613–1640 行 — `DisConnectDev` 将客户端传入的数字/字符串直接 `as DEVHANDLE`，确认 RES-01 风险
- `src/main.rs` 3505–3680 行 — `DigestInit` 的 `hash_{id}` 确定性、`DigestUpdate`/`CloseHash` 的按次 `Library::new`
- `src/main.rs` 1885–1935 行 — `CheckPIN` 以 `provider/dev/app` 为键缓存明文 PIN；失败消息含重试次数
- `src/main.rs` 238–330 行 — `run_server` 打印请求地址而非 `local_addr()`，确认临时端口不可用
- `src/main.rs` 48–155 行 — `get_lib_path`/`get_api`/`expand_env_vars` 现状
- `Cargo.toml` — 无 `[lib]` 段；tokio `full` feature 已启用；Windows 依赖已按 target 门控
- `src/win_service.rs` — 调用 `crate::run_server(Some(stop_rx))`，拆分后路径需更新
- `packaging/windows/build.ps1` — 期望路径 `target\i686-pc-windows-msvc\release\skf-service.exe`；产物名不得改变
- `.planning/codebase/*.md` — 代码库地图（架构、结构、约定、测试、风险）
- `.planning/research/*.md` — 项目级研究（stack 取舍、陷阱清单、阶段顺序理由）

### Secondary (MEDIUM confidence)
- 构建产物命名推导：包名 `skf-service` + 新增 `src/lib.rs` ⇒ lib `skf_service`、bin `skf-service`，Windows 产物 `skf-service.exe` 不变。该结论由 Cargo 既有约定推得，**未在本机执行 Windows 目标构建验证**；计划中应以 `cargo check --target i686-pc-windows-gnu` 作为机器可验证的替代证据。

### Tertiary (LOW confidence — needs validation)
- 外部 crate 版本时效性（本轮未联网核实；且本阶段按 D-21 预期不新增运行时依赖，影响有限）。
- `tempfile` 是否真的必要：可能完全不需要，取决于计划是否出现真实文件系统状态需求。

</sources>

<metadata>
## Metadata

**Research scope:**
- Core technology: Rust 二进制→库 crate 拆分、FFI provider 接缝、黄金夹具契约冻结
- Ecosystem: 仅既有依赖；不引入新 crate（D-21）
- Patterns: 薄二进制、领域 trait + Drop 守卫、可注入失败的 fake、录制期冻结易变字段、临时端口端到端重放
- Pitfalls: 不可失败的夹具、硬件依赖导致的录制停滞、绑定行为漂移、库按次加载、搬运中夹带行为变更、Windows 构建断裂、测试依赖环境

**Confidence breakdown:**
- Standard stack: HIGH —— 结论是"不新增依赖"，由 D-21 与本仓库现状共同确定
- Architecture: HIGH —— 分层与接缝由现有代码结构与阶段 2/3 的既有决策推导
- Pitfalls: HIGH —— 每条都对应本仓库可复核的具体证据（计数、代码行、现有模式）
- Code examples: MEDIUM —— Rust 惯用法与既有代码可直接对应；产物命名推导未在本机 Windows 目标验证

**Research date:** 2026-09-11
**Valid until:** 阶段 1 结束（本研究的结论全部基于仓库当前状态，仓库一旦重构即需重读源码而非依赖本文档）
</metadata>

---

*Phase: 01-structural-foundation-and-test-seam*
*Research completed: 2026-09-11*
*Ready for planning: yes*

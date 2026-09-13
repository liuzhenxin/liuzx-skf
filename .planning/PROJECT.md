# LiuZX SKF Service

## What This Is

LiuZX SKF Service 是一个 Rust 编写的本地 SKF 网关，通过 WebSocket JSON-RPC 风格接口统一访问不同厂商的 USB Key/HSM。它向浏览器和本地应用提供设备管理、PIN 验证、证书、签名、哈希、加解密等能力，并可作为 Windows 原生服务长期运行。

当前正式分发重点是 Windows x64 操作系统上的 GM3000：由于厂商 DLL 为 PE32/i386，服务进程以 i686 运行并通过 WoW64 部署。

## Core Value

应用能够通过稳定、安全且与厂商实现解耦的统一接口访问 USB Key 的硬件密码能力。

## Milestone Status

**No active milestone.** v0.3.0 Production Hardening (2026-09-12) and v0.4.0 Secure
Remote Operation (2026-09-13) are shipped and archived in
`.planning/milestones/`. Start the next milestone with `$gsd-new-milestone`.

**v0.4.0 delivered:** optional TLS termination, mTLS / bearer-token client
authentication, a single non-loopback exposure rule (opt-in + TLS + client auth),
and a structured non-sensitive authorization audit — with the default still
plaintext loopback and the v0.2.0 client contract unchanged.

**Explicitly deferred to a later milestone:** per-device concurrency/throughput,
multi-vendor (FishMan/3000GM) and Linux/macOS distribution, remote log shipping /
metrics / health endpoint, and real CA integration for `IssueCertificate`.

## Requirements

### Validated

- ✓ v0.2.0 的 37 个 JSON-RPC 方法的请求/响应契约已被夹具冻结，并在每次改动后端到端重放校验 — Phase 1（`tests/fixtures/v0.2.0/`、`tests/contract_fixtures.rs`）
- ✓ 服务可拆为 library + thin binary，42 个测试在无硬件、无驱动、无网络下运行 — Phase 1（`src/lib.rs`）
- ✓ SKF 调用具备可替换的 provider 抽象，原生实现只加载一次厂商库，fake 可注入五类失败 — Phase 1（`src/provider/`）
- ✓ 服务可在临时端口上启动并回报真实绑定地址，支持端到端自动化 — Phase 1（`src/server/mod.rs`）
- ✓ 服务可以通过 WebSocket JSON-RPC 风格接口暴露设备、应用、容器、证书和密码操作 — existing/v0.2.0
- ✓ 服务可以根据 `config/skf.yaml` 在运行时加载不同平台、不同厂商的 SKF 动态库 — existing
- ✓ 已支持 SM2、SM3、SM4、RSA 相关的密钥、签名、验签、哈希及加解密流程 — existing
- ✓ 已提供 Promise 风格 JavaScript 客户端、静态 API Demo 和物理 USB Key 集成脚本 — existing
- ✓ Windows x64 可分发 i686 GM3000 独立包，并可注册为自动启动的 Windows SCM 服务 — v0.2.0
- ✓ GitHub 标签可自动构建并发布经过 PE32/i386 架构校验的 Windows ZIP — v0.2.0
- ✓ 每个客户端会话拥有独立授权状态（`OsRng` 会话 ID、TTL 由 `SKF_AUTH_TTL_SECONDS` 控制），断开/超时/设备不可用三条路径都会清除授权，且不保留可恢复的明文 PIN — Phase 2（`src/session/`、`cargo test session`）
- ✓ 客户端只接触服务生成的不透明句柄（`dev-N`/`app-N`/`cnt-N`/`hsh-N`）；伪造整数、未知、过期或类型错误的句柄一律被拒 — Phase 2（`src/session/handles.rs`、`tests/session_isolation.rs`）
- ✓ 设备、应用、容器和流式摘要资源通过守卫 RAII 在成功、原生错误、断连和会话结束场景确定性释放 — Phase 2（`src/domain/`、`tests/session_lifecycle.rs`）
- ✓ 协议入口限制帧（1 MiB）、载荷（256 KiB，decode 前）与并发连接（64），非 wait 请求有 30s 超时 — Phase 3（`tests/transport_limits.rs`、`tests/ffi_serialization.rs`）
- ✓ 阻塞厂商调用（含守卫析构）通过每请求 `spawn_blocking` 执行在阻塞池上，async worker 不再被占用 — Phase 3（`src/main.rs`、`src/provider/native.rs`）
- ✓ 同一 provider 库的原生调用由一把全局锁串行化，wait/cancel 显式豁免；全 crate 仍仅各一处 `unsafe impl Send/Sync` — Phase 3（`tests/ffi_serialization.rs`、`tests/provider_invariants.rs`）
- ✓ 非回环 WebSocket 绑定在 `bind` 前被拒，除非 YAML `allow_remote` 或 `SKF_ALLOW_REMOTE` 显式 opt-in — Phase 3（`tests/loopback_gate.rs`）
- ✓ 全部 37 个方法具有只读/破坏性分类与文档，且不接入请求路径 — Phase 3（`src/domain/classification.rs`、`docs/OPERATION-CLASSIFICATION.md`）
- ✓ 全仓储 `cargo fmt --all -- --check` 通过（隔离的 formatting-only 提交），且未引入 `rustfmt.toml` — Phase 4（`04-01-SUMMARY.md`）
- ✓ `cargo clippy --all-targets -- -D warnings` 零 warning；唯一保留的 allow 限于镜像 SKF C ABI 的两个函数且带注释；工具链由 `rust-toolchain.toml` 固定 1.93.0 — Phase 4（`src/skf/api.rs`、`rust-toolchain.toml`）
- ✓ CI 在 PR 与 `main`/`dev` 推送时运行 fmt、clippy、硬件无关测试、x86_64 macOS 全量（含 37 夹具契约重放）与 i686 编译检查，并用 `gate-selftest` 证明门禁对失败测试敏感 — Phase 4（`.github/workflows/ci.yml`、`docs/CI.md`）
- ✓ 服务逐阶段上报 `StartPending`，仅在监听器绑定且配置解析后报 `Running`；启动失败以 `ServiceSpecific(1..4)` 退出并写状态文件，`status` 报告阶段 — Phase 5（`src/service_state.rs`、`src/win_service.rs`、`docs/WINDOWS-SERVICE.md`）
- ✓ 安装器对安装目录施加并校验限制性 ACL（SYSTEM/Administrators 全控，Users 仅 RX）；升级/卸载轮询等待进程退出与文件解锁，不留下残留注册或锁定文件 — Phase 5（`packaging/windows/install.ps1`、`uninstall.ps1`、`verify-service.ps1`）

- ✓ 日志结构化（JSON Lines）、分级、按 10 MiB 轮转并保留 7 个文件；扫描测试保证不含 PIN/密钥/解密载荷 — Phase 6（`src/logging.rs`、`tests/log_redaction.rs`）
- ✓ 本地 `skf-service diagnose [--json]` 报告 config/provider/库文件/库加载/监听器五阶段与运行中状态，只输出非敏感事实（不含库路径） — Phase 6（`src/diagnostic.rs`）
- ✓ 协议接受可选 `apiVersion`，新增 `GetProtocolVersion`（文档化增补），v0.2.0 客户端无需修改；受限方法可经 `SKF_RESTRICT_LEGACY=1` 返回文档化 `-100` — Phase 6（`tests/protocol_version.rs`、`tests/restricted_methods.rs`）
- ✓ 发布 ZIP 带 SHA-256，workflow 校验 checksum 并解包断言内容与 exe/DLL `Machine=0x014C` — Phase 6（`packaging/windows/build.ps1`、`release-windows.yml`）
- ✓ 威胁模型、中英对照会话/限制文档、发布说明与 agent 文档已补齐 — Phase 6（`THREAT-MODEL.md`、`docs/SESSION-AND-LIMITS.md`、`RELEASE-NOTES.md`）
- ✓ WebSocket 监听器可选启用 TLS（`tls.cert_file`/`tls.key_file`，`rustls` + 固定 `ring` provider，最低 TLS 1.2）；未配置时保持明文回环，证书/私钥加载失败为致命配置错误（退出码 1），绝不回退明文 — Phase 7（`src/server/mod.rs`、`tests/tls.rs`）
- ✓ 客户端认证支持 `client_auth: none|mtls|token`：mTLS 用 `WebPkiClientVerifier` 校验客户端证书，token 在 WebSocket 升级前校验 `Authorization: Bearer` 并返回 401；令牌来自 `SKF_TLS_TOKEN` 或 `token_file`，常量时间比较 — Phase 8（`src/client_auth.rs`、`tests/client_auth.rs`）
- ✓ 非回环绑定仅在 opt-in **且** TLS **且** 客户端认证三者齐备时放行，拒绝发生在 bind 之前（退出码 3）并列出缺失项；回环默认不变 — Phase 9（`src/server/mod.rs`、`tests/loopback_gate.rs`）
- ✓ 授权决策以结构化非敏感事件记录（granted/denied 带 reason/expired/device.unavailable 带 cleared），事件类型无法携带 PIN/密钥/载荷/令牌 — Phase 9（`src/audit.rs`、`tests/audit.rs`）
- ✓ 已认证身份（mTLS CN / token / anonymous）接入会话供审计；私钥与令牌不进入日志、`diagnose`、状态文件或发布元数据 — Phase 8（`src/server/mod.rs`、`src/main.rs`）
- ✓ 威胁模型、会话/限制、Windows 服务与发布说明文档描述 TLS/auth/bind/审计与迁移路径；CI 快速任务纳入 hermetic 安全套件；阶段 3–6 Nyquist 签核关闭 — Phase 10（`THREAT-MODEL.md`、`docs/CI.md`、`10-NYQUIST-AUDIT.md`）

### Active

- *(none — v0.4.0 scope complete; see Deferred and the roadmap backlog)*

### Out of Scope

- 公网或跨不可信网络直接暴露当前 WebSocket API — 在完成认证与加密传输设计前保持本机优先
- 将 `IssueCertificate` 建设为生产 CA — 它仍是测试/Mock 能力，生产证书应由外部合规 CA 签发
- v0.3.0 内完成全部 FishMan、3000GM、Linux 和 macOS 正式分发 — 本版本优先稳定 GM3000 Windows 基线
- 获取商业代码签名证书 — 可预留签名步骤并生成校验和/SBOM，但证书采购是外部流程
- 无兼容层地重写或删除 v0.2.0 客户端方法 — 安全风险方法可收紧，但必须提供明确迁移说明
- 在标准托管 CI 中模拟真实 GM3000 硬件 — 无硬件测试使用 Fake provider，真实设备由独立 UAT 验证

## Context

- v0.2.0 已发布 Windows x64-host / GM3000 x86 独立包，包含便携运行、SCM 安装、开机自启和 GitHub Release 自动构建。
- 代码库地图位于 `.planning/codebase/`；`src/main.rs` 现已收缩为组合根 + `Session` 实现 + 14 个未迁移分支（约 1,990 行），19 个安全相关分支已迁入 `src/domain/`，协议类型与类型化参数提取在 `src/protocol/`。
- 一个进程级 `Arc<SkfContext>` 被所有 WebSocket 连接共享，但只保留 `config`、`provider` 与已加载库缓存；明文 PIN 与流式哈希句柄缓存已按会话迁移到 `SessionState`。
- FFI 边界由 `src/skf/api.rs` 与 `src/skf/types.rs` 提供，依赖厂商 C ABI、原始指针和若干显式安全假设。
- Phase 3 后，每个请求的 dispatcher 整体运行在 Tokio 阻塞池上，厂商调用与守卫析构都不占用 async worker；所有经守卫的 native 调用共享一把每 provider 全局锁（wait/cancel 豁免），非 wait 请求有 30s 超时。当前 `cargo test` 运行 133 个 Rust 测试。
- 当前网络 API 支持可选 TLS（`rustls` + 固定 `ring`）以及 `client_auth: mtls|token`；非回环绑定需 opt-in + TLS + 客户端认证三者齐备，默认仍为明文回环。
- 授权决策（granted/denied/expired/device-unavailable）从 `SessionState` 单一决策点以结构化 JSON 记录，字段不含 PIN/密钥/载荷/令牌（`src/audit.rs`）。
- 当前 `cargo test` 在无硬件下运行 145 个库测试，另含 TLS/client-auth/audit 等 hermetic 集成套件；附 GM3000 令牌时 37 个 v0.2.0 夹具全部回放通过。
- Windows Release 工作流能构建并检查架构，但尚未自动验证服务安装、启动就绪、停止、升级或卸载。
- Windows 服务现按阶段上报启动状态（`service-state.json`），并以分类非零退出码触发 `sc failure` 重启；安装器限制安装目录 ACL 并在升级/卸载时等待文件解锁。SVC 的 SCM 实机行为仍需在 Windows VM 上按 `packaging/windows/verify-service.ps1` 验证。
- `.github/workflows/ci.yml` 已是 PR/主分支门禁：fmt、clippy(-D warnings)、硬件无关测试、x86_64 macOS 全量（含契约重放）、i686 编译检查、门禁 self-test。真正阻断合并仍需仓库分支保护设置（见 `docs/CI.md`）。
- `rust-toolchain.toml` 固定 Rust 1.93.0；本地与 CI 的 fmt/clippy 基线因此一致。
- `IssueCertificate(double=true)` 的测试加密证书目前不保证与生成的加密私钥匹配，不应视为生产证书流程。

## Constraints

- **Compatibility**: Windows x64 上的 GM3000 组合必须保持 `skf-service.exe=PE32/i386` 与 `mtoken_gm3000.dll=PE32/i386` — 64 位进程无法加载现有厂商 DLL
- **Hardware**: GM3000 驱动和物理 USB Key 不存在于标准 GitHub Hosted Runner — CI 必须区分 Fake provider 验证和真实硬件 UAT
- **Security**: PIN、私钥、会话密钥和明文业务数据不得进入日志、规划文档或发布元数据 — PKI/密码设备数据均按敏感信息处理
- **FFI**: SKF ABI 名称和 C 布局必须保持规范兼容 — 重构不能随意修改结构大小、对齐、调用约定或符号名
- **API**: v0.2.0 JavaScript 客户端是现有兼容基线 — 协议加固应提供版本策略和迁移路径
- **Operations**: 正式 Windows 包运行时不得要求 Rust、Cargo 或 Node.js — OpenSSL 仅允许作为 Mock 证书接口的可选依赖
- **Privileges**: 服务权限应最小化，但必须先验证厂商驱动在目标账户下可访问 — 不能以降低权限为名破坏硬件可用性

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| v0.3.0 聚焦生产加固而非新增大量 SKF 方法 | 当前风险集中在会话、句柄、测试和运维边界，继续扩展单体分发器会放大风险 | — Pending |
| GSD 规划独立存放于 `liuzx-skf/.planning/` | 父级 PKI 工作区另有规划，独立根可避免跨仓库状态和提交污染 | ✓ Good |
| 保持 Windows x64 OS + i686 服务进程 | GM3000 Windows DLL 为 PE32/i386，这是不可绕过的 ABI 约束 | ✓ Good |
| 无硬件 CI 使用 Fake provider，真实设备使用独立 UAT | 托管 CI 无法可靠提供驱动和物理设备，两层验证可兼顾速度与真实性 | — Pending |
| 默认保持本机访问，不在 v0.3.0 宣称公网服务能力 | 当时协议无 TLS、客户端认证或授权，扩大网络暴露不可接受 | ⚠️ Revisit — v0.4.0 以 TLS + 客户端认证 + opt-in 三重控制取代了纯回环边界 |
| 远程暴露必须同时具备 opt-in、TLS 与客户端认证 | 任何单一控制都不足以在不可信网络上保护 PKI 设备 | ✓ Good |
| TLS provider 显式固定为 `ring` | 默认 `aws-lc-rs` 会破坏 i686 Windows 交叉编译（发布目标） | ✓ Good |
| 审计只在 `SessionState` 单一决策点发射 | 授权决策全部汇聚于此，避免逐调用点遗漏 | ✓ Good |
| 保留 v0.2.0 客户端兼容层并引入明确协议版本 | 安全重构不能静默破坏现有集成 | ✓ Good |
| `IssueCertificate` 保持 Mock 定位 | 生产 CA 涉及合规、密钥保护和证书策略，不属于本地 SKF 网关职责 | ✓ Good |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `$gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `$gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-09-13 after v0.4.0 (Secure Remote Operation) — milestone complete*

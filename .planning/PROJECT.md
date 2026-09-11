# LiuZX SKF Service

## What This Is

LiuZX SKF Service 是一个 Rust 编写的本地 SKF 网关，通过 WebSocket JSON-RPC 风格接口统一访问不同厂商的 USB Key/HSM。它向浏览器和本地应用提供设备管理、PIN 验证、证书、签名、哈希、加解密等能力，并可作为 Windows 原生服务长期运行。

当前正式分发重点是 Windows x64 操作系统上的 GM3000：由于厂商 DLL 为 PE32/i386，服务进程以 i686 运行并通过 WoW64 部署。

## Core Value

应用能够通过稳定、安全且与厂商实现解耦的统一接口访问 USB Key 的硬件密码能力。

## Current Milestone: v0.3.0 Production Hardening (Phase 1 of 6 complete)

**Goal:** 在保持 v0.2.0 GM3000 接口和部署兼容性的同时，建立安全会话、受控句柄、可测试架构以及可观测、可恢复的 Windows 服务运行基础。

**Target features:**
- 将 PIN 授权和原生资源限定在客户端会话内，避免跨连接复用明文 PIN 或客户端直接控制原生指针。
- 拆分单体请求分发器，建立可替换 SKF provider 接口和无硬件测试后端，为 Rust 单元/协议测试与 CI 提供基础。
- 完善 Windows 服务的启动状态、健康诊断、失败退出、日志轮转和安装包冒烟验证。
- 为请求大小、并发、超时、危险操作及本地访问边界建立明确、可测试的安全策略。
- 增强 Release 完整性信息，同时保留真实 GM3000 硬件 UAT 作为独立验证层。

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

### Active

- [ ] 协议入口限制连接、帧、载荷、并发和执行时间，并区分只读与危险操作
- [ ] SKF 调用通过清晰的 provider/worker 边界执行，阻塞厂商调用不会占满 Tokio 异步工作线程
- [ ] 核心协议和业务流程可以使用 Fake SKF 后端在无硬件 CI 中确定性测试
- [ ] Windows SCM 仅在监听器就绪后报告 Running，启动失败返回非零状态并触发恢复策略
- [ ] 操作员可以通过不泄露敏感信息的健康诊断了解配置、端口、provider 和驱动状态
- [ ] Windows 服务日志有级别、关联信息、轮转和保留策略，不会无限增长
- [ ] Release 产物包含可校验的摘要和构建元数据，并执行包结构与启动冒烟检查

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
- 当前 `cargo test` 运行零个 Rust 测试；Node 集成脚本依赖真实设备、驱动、PIN 和 OpenSSL，无法作为普通 Pull Request CI。
- Windows Release 工作流能构建并检查架构，但尚未自动验证服务安装、启动就绪、停止、升级或卸载。
- `IssueCertificate(double=true)` 的测试加密证书目前不保证与生成的加密私钥匹配，不应视为生产证书流程。
- 当前网络 API没有传输认证；服务模式默认回环地址是重要的临时安全边界。

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
| 默认保持本机访问，不在 v0.3.0 宣称公网服务能力 | 当前协议无 TLS、客户端认证或授权，扩大网络暴露不可接受 | — Pending |
| 保留 v0.2.0 客户端兼容层并引入明确协议版本 | 安全重构不能静默破坏现有集成 | — Pending |
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
*Last updated: 2026-09-11 after Phase 2 (Session Authorization and Resource Ownership)*

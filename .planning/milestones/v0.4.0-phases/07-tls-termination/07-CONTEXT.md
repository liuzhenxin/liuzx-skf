# Phase 7: TLS Termination - Context

**Gathered:** 2026-09-12
**Status:** Ready for planning

<domain>
## Phase Boundary

让 WebSocket 监听器**可以选择性地**以 TLS 提供服务，且默认行为与 v0.2.0 明文回环完全一致。

交付范围（TLS-01..04, COMPAT-01）：

1. 可配置的服务端证书/私钥；加载失败则启动失败。
2. TLS 客户端完成握手后，现有方法正常工作。
3. **默认不启用 TLS**；不配置时保持明文（向后兼容）。
4. 私钥/证书材料绝不进入日志、`diagnose`、状态文件或发布元数据。

**本阶段不做**：客户端认证（mTLS/token，阶段 8）；非回环绑定收紧（阶段 9）；授权审计（阶段 9）；文档收尾与 Nyquist（阶段 10）；HTTP demo 的 TLS。

**硬约束**：37 个 v0.2.0 夹具必须继续匹配（默认明文回环）；`cargo check --target i686-pc-windows-gnu` 保持通过（`ring` provider）；密钥不落入任何非内存位置。
</domain>

<decisions>
## Implementation Decisions

### 配置（区域 1）
- **D-01:** 新增**具名** `tls:` 配置块（`SkfConfig` 的 flatten 决定必须是具名字段，D-07 约束）：
  ```yaml
  tls:
    cert_file: "config/server.crt"   # PEM 链
    key_file: "config/server.key"    # PEM 私钥（敏感）
    client_auth: none                # none | mtls | token（本期只接受 none）
  ```
- **D-02:** 支持环境变量覆盖 `SKF_TLS_CERT` / `SKF_TLS_KEY` / `SKF_TLS_CLIENT_AUTH`，便于运维与测试。
- **D-03:** 证书/私钥路径**相对 exe 目录**解析（与 `config/`、`api/` 一致）；**不支持内联 PEM**（私钥不得写入 YAML）。

### TLS 默认值与 client_auth（区域 2）
- **D-04:** TLS **默认关闭**；启用时使用 `rustls` + **`ring` provider** 的安全默认套件，最低 **TLS 1.2**。
- **D-05:** `tls.client_auth` 字段本期引入，但**只接受 `none`**；配置 `mtls`/`token` 时启动**报错 "client_auth not yet supported"**（不静默降级）；阶段 8 实现两种模式。

### 错误归属与可观测（区域 3）
- **D-06:** 证书/私钥加载失败归入 `StartupError::Config`（启动退出码 **1**）；错误消息只含类别（如 "TLS certificate could not be loaded"），**不含路径与密钥内容**。
- **D-07:** `diagnose` 在本阶段增加**非敏感布尔**：`tls_enabled`、`tls_cert_loaded`；不输出路径或任何密钥材料。状态文件保持不变（不含 TLS 材料）。

### 测试证书策略（区域 4）
- **D-08:** 新增 **`rcgen` 作为 dev-dependency**，测试内生成自签服务端证书（并为阶段 8 预留生成客户端 CA）。理由：hermetic、CI 可跑、不依赖 OpenSSL CLI 文本（避免重蹈 `IssueCertificate` 的版本漂移）。
- **D-09:** 不签入私钥；不使用 `openssl` CLI 的输出来断言。

### 实现形态（研究结论）
- **D-10:** 服务端自行用 `tokio_rustls::TlsAcceptor` 包住 `TcpStream`，再把 `TlsStream` 交给现有 `accept_async_with_config`/`accept_hdr_async_with_config`；**不启用** `tokio-tungstenite` 的 TLS feature（那是客户端连接侧）。
- **D-11:** `admit`/`spawn_client` 泛型化到 `AsyncRead + AsyncWrite + Unpin + Send`（或使用装箱流），使明文与 TLS 共用同一请求循环；连接信号量许可必须**覆盖 TLS 握手**，握手失败时释放。
- **D-12:** 握手失败只记录错误类别并丢弃连接，**accept 循环不中断**。
- **D-13:** TLS 为**纯增量**：默认明文回环不变，37 夹具无需改动。

### the agent's Discretion
- `TlsConfig` 的具体类型与反序列化（可选字段的默认值/校验）。
- 泛型 vs 装箱流的具体选择。
- `rcgen` 版本与测试辅助函数的组织。
- 启动日志中 TLS 相关字段的措辞（不得含路径/密钥）。
- 是否在启动时打印 "TLS enabled" 一行（建议是，仅布尔与是否提供客户端认证）。
</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### 阶段范围与验收
- `.planning/ROADMAP.md` §Phase 7 — 目标、依赖、4 条成功标准
- `.planning/REQUIREMENTS.md` — TLS-01..04、COMPAT-01
- `.planning/research/STACK.md` / `ARCHITECTURE.md` / `PITFALLS.md` — 已核对的 crate 版本与集成/陷阱
- `.planning/PROJECT.md` — 里程碑目标与明确推迟项

### 前序阶段约束
- `.planning/milestones/v0.3.0-phases/05-windows-service-reliability/05-CONTEXT.md` — 启动错误分类与退出码
- `.planning/milestones/v0.3.0-phases/06-observability-diagnostics-and-release-verification/06-CONTEXT.md` — 日志脱敏、`diagnose` 非敏感输出
- `docs/CI.md` / `docs/WINDOWS-SERVICE.md` / `docs/SESSION-AND-LIMITS.md` / `THREAT-MODEL.md` — 现有边界文档

### 现有源码（落点）
- `src/server/mod.rs` — `bind_with_progress`/`bind`、`serve`、`admit`、`spawn_client`、`ServerOptions`、`StartupError`
- `src/config/mod.rs` — `SkfConfig`（flatten 约束）、`load`
- `src/diagnostic.rs` — 非敏感报告
- `src/logging.rs` — 结构化 JSON 日志与脱敏
- `src/service_state.rs` — 启动状态与退出码
- `Cargo.toml` / `rust-toolchain.toml` — 依赖与工具链
- `tests/loopback_gate.rs` / `tests/transport_limits.rs` / `tests/session_support/mod.rs` — 传输与启动测试辅助

### 外部依赖（已在本地取回并核对）
- `rustls 0.23`、`tokio-rustls 0.26`、`rustls-pemfile 2`、`rustls-pki-types 1`；dev: `rcgen`

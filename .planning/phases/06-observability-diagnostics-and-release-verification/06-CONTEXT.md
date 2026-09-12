# Phase 6: Observability, Diagnostics, and Release Verification - Context

**Gathered:** 2026-09-11
**Status:** Ready for planning

<domain>
## Phase Boundary

让运维人员能够诊断问题而不泄露敏感信息，并让发布产物可被独立校验。这是 v0.3.0 里程碑的最后一阶段（OBS-01..04、REL-01..05、DOC-01..03，共 12 项）。

具体交付：

1. **OBS-01/02**：结构化、分级、按大小轮转并保留有界的日志；任何日志不含 PIN、私钥、会话密钥或解密载荷。
2. **OBS-03/04**：本地诊断命令，报告配置/ provider / 库文件 / 库加载 / 监听器绑定五阶段，且只输出非敏感非标识事实。
3. **REL-01/02**：协议接受版本指示且 v0.2.0 客户端无需修改；为安全受限的方法仍然存在并返回文档化错误。
4. **REL-03/04/05**：发布 ZIP 带 SHA-256 校验文件；workflow 解包校验内容与 `Machine=0x014C`；发布说明记录每项有意行为变更与迁移路径。
5. **DOC-01/02/03**：威胁模型、中英文会话/限制/受限方法文档、面向 agent 的项目文档更新到重构后布局。

**本阶段不做**：新增 SKF 方法能力；改变已冻结的 v0.2.0 契约（唯一例外是文档化的**增补**：`GetProtocolVersion` 与可选 `apiVersion` 字段）；传输层 TLS（仍本机优先）。

**硬约束**：37 个 v0.2.0 夹具必须继续匹配；`EXPECTED_METHODS` 的任何增补都必须是显式、文档化的契约增补。

</domain>

<decisions>
## Implementation Decisions

### 日志（OBS-01 / OBS-02，区域 1）
- **D-01:** 保留 `log` facade，新增 **`flexi_logger`**（单依赖）负责轮转与保留。不迁移到 `tracing`（改动面过大）；现有 `log::*` 调用点保持。
- **D-02:** 新增 `src/logging.rs`（跨平台）：
  - `init_console()`：前台模式，输出到 stderr，`RUST_LOG` 过滤（保留 `env_logger` 的 `default_filter_or("info")` 语义）；
  - `init_service_file(dir)`：服务模式，写入 `<dir>/logs/skf-service.log`，**按大小轮转 10 MiB，保留 7 个文件**；
  - 结构化格式：**JSON Lines**（每行一个 `serde_json` 对象，字段 `ts`/`level`/`target`/`message`），用 flexi_logger 自定义 `LogFormat` 实现；`serde_json` 已是依赖。
- **D-03:** Windows 服务的 `redirect_stdio_to_log` 改为写 **`skf-service.console.log`**（独立文件），避免与 flexi_logger 的 `skf-service.log` 混写。它仍用于捕获 panic/`println!`。
- **D-04（脱敏，结构性）:** 两层机制：
  1. `logging::sanitize(value: &str) -> String`：去除控制字符并截断到 512 字符；任何要把外部值写进日志的地方使用它；
  2. **源码扫描测试** `tests/log_redaction.rs`：扫描 `src/**/*.rs`，若日志宏（`log::info!`/`warn!`/`error!`/`debug!`/`trace!` 及 `println!`）的参数列表中直接出现敏感标识符（`pin`、`pin_str`、`key_bytes`、`sym_key`、`private`、`decrypted`、`req.params`、`cert_bytes`）则失败，除非该行带有显式 `// redaction-allow:` 注释并说明理由。
- **D-05:** 明确不在日志中记录请求参数原文；现有调用点均满足，测试防止未来漂移。

### 诊断（OBS-03 / OBS-04，区域 2）
- **D-06:** 新增跨平台子命令 **`skf-service.exe diagnose [--config <path>] [--json]`**（在 `main()` 中于 Windows SCM 分派之前处理，所有平台可用）。
  检查项与输出字段（均为非敏感）：
  - `config_loaded`（bool）、`config_error_class`（概览，不含路径细节之外的堆栈）
  - `provider_alias`（配置的默认别名）
  - `provider_resolved`（bool：当前 OS 是否有路径配置）
  - `library_file_present`（bool）
  - `library_loaded`（bool：实际尝试加载，失败只报错误类别，不打印完整路径之外的细节；`provider_load_failed` 的库路径暴露问题一并处理，见 D-07）
  - `listener_bindable`（bool：用配置的 WS 地址绑定一个探针端口后立即释放）
  - 若 `service-state.json` 存在：`state`、`stage`、`code`、`address`、`uptime_seconds`、`pid`
- **D-07:** 诊断输出**只**包含阶段布尔、provider 别名、端口、uptime、错误类别。**不**输出库完整路径、配置值、PIN、密钥。顺带处理阶段 2 遗留的 L-05（`provider_load_failed` 消息暴露库路径）：在 `diagnose` 中对路径做 `sanitize`（仅显示文件名），或在错误类别中不包含路径。
- **D-08:** `service_state` 新增 `started_at`（epoch 秒），在首次 `StartPending` 写入，`Running`/`Failed` 保留；`diagnose` 据此计算 `uptime_seconds`。

### 协议版本与受限方法（REL-01 / REL-02，区域 3）
- **D-09:** `RpcRequest` 新增可选字段 `api_version: Option<u32>`（`#[serde(default, rename = "apiVersion")]`）。v0.2.0 客户端不发送该字段 → `None` → 视为 `1`。
  - 缺失或 `<= CURRENT_API_VERSION(=1)`：正常处理；
  - `> CURRENT_API_VERSION`：返回 `-1`，消息 `Unsupported apiVersion {n}; this service supports up to 1`（文档化）。
- **D-10:** 新增方法 **`GetProtocolVersion`**，返回 `{"min":1,"current":1,"service":"0.3.0"}`。这是**文档化的契约增补**：更新 `tests/common/mod.rs::EXPECTED_METHODS` 增补该方法，并新增 `ADDITIVE_METHODS` 白名单，使 `every_expected_method_has_a_fixture` 对增补方法不要求 v0.2.0 夹具；增补事实写入 `RELEASE-NOTES.md` 与夹具 README。
- **D-11:** 受限方法集合为 `IssueCertificate`（Mock）与 `Transmit`（原始透传）。**默认行为不变**（保持 37 夹具）。当环境变量 **`SKF_RESTRICT_LEGACY=1`** 时，这些方法返回文档化错误 `-100`，消息 `Method {name} is restricted in this deployment; see RELEASE-NOTES.md`。`GetProtocolVersion` 与诊断不受影响。
- **D-12:** 受限集合与错误码在 `src/domain/classification.rs`（或相邻模块）中以常量表达，供文档与测试引用；不改变 `OperationClass`。

### 发布完整性（REL-03 / REL-04 / REL-05，区域 4）
- **D-13:** `packaging/windows/build.ps1` 在生成 ZIP 后写 `dist/skf-service-windows-x64-gm3000-x86.zip.sha256`，内容为 `sha256sum` 风格 `<lowercase-hex> *<zip-name>`。
- **D-14:** `.github/workflows/release-windows.yml` 增加：
  - 校验 `.sha256` 与 `Get-FileHash -Algorithm SHA256` 一致；
  - 解包 ZIP 到临时目录，断言 `skf-service.exe`、`mtoken_gm3000.dll`、`config\skf.yaml`、`api\` 齐全；
  - 对 exe 与 DLL 断言 `Machine=0x014C`（复用 `build.ps1` 的 `Get-PeMachine` 逻辑或等价 PowerShell）；
  - 上传 ZIP 与 `.sha256` 两个 artifact，tag 发布时两个附件都上传。
- **D-15:** 新增根目录 `RELEASE-NOTES.md`，包含：v0.3.0 有意行为变更（不透明句柄、非回环默认拒绝、帧/载荷/连接/超时限制、服务退出码、provider 降级 vs 致命、受限方法），每项附迁移路径；以及新增能力（`diagnose`、`GetProtocolVersion`/`apiVersion`、分类、ACL、checksum、结构化轮转日志）。
- **D-16:** `Cargo.toml` 版本升至 **`0.3.0`**；`GetProtocolVersion` 的 `service` 取自 `CARGO_PKG_VERSION`。

### 文档（DOC-01 / DOC-02 / DOC-03，区域 4）
- **D-17:** `THREAT-MODEL.md`（仓库根）：信任边界（本机回环、无 TLS、无客户端认证）、local-only 暴露决策与 `allow_remote`、不保留 PIN 的决策与依据、限制（帧/载荷/连接/超时/串行化）、受限方法、已知限制（HTTP demo 仍可绑非回环、`IssueCertificate` 为 Mock、厂商 DLL 线程安全未证、锁/超时为"调用方止损"）。
- **D-18:** `docs/SESSION-AND-LIMITS.md`：**中英对照**，描述会话/授权模型（会话 ID 不下发、TTL、断连/拔出/超时清除、`CheckPIN` 只存 deadline）、限制数值、受限方法与 `SKF_RESTRICT_LEGACY`。
- **D-19:** 更新 `README_CN.md` / `README_EN.md` / `AGENTS.md` / `CLAUDE.md`，反映重构后模块布局（`protocol/`、`domain/`、`session/`、`provider/`、`service_state/`、`logging/`、`diagnostic/`）与当前命令（`cargo fmt/clippy/test`、`cargo check --target i686-pc-windows-gnu`、`skf-service.exe diagnose`、`packaging/windows/verify-service.ps1`、CI 说明）。

### the agent's Discretion
- `flexi_logger` 的具体版本与 API 形态（`rotate`/`FileRotation`/保留参数名随版本而异，实现时以 registry 源码为准）。
- JSON Lines 的精确字段名与转义实现。
- `diagnose` 的文本与 `--json` 两种呈现的排版。
- `ADDITIVE_METHODS` 在测试中的组织方式。
- `RELEASE-NOTES.md` 与 `THREAT-MODEL.md` 的章节顺序。

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### 阶段范围与验收标准
- `.planning/ROADMAP.md` §Phase 6 — 目标、依赖、5 条成功标准
- `.planning/REQUIREMENTS.md` §Observability / §Release / §Documentation — OBS-01..04、REL-01..05、DOC-01..03
- `.planning/PROJECT.md` — Constraints、Out of Scope、里程碑目标

### 前序阶段产出（本阶段直接依赖）
- `.planning/phases/05-windows-service-reliability/05-CONTEXT.md` + `05-VERIFICATION.md` — 状态文件、退出码、`docs/WINDOWS-SERVICE.md`
- `.planning/phases/04-format-lint-normalization-and-ci-gate/04-VERIFICATION.md` — CI 门禁与工具链
- `.planning/phases/03-transport-hardening-and-concurrency/03-VERIFICATION.md` — 限制数值与锁
- `.planning/phases/02-session-authorization-and-resource-ownership/02-CONTEXT.md` — 会话/句柄模型；L-05（库路径暴露）在本阶段处理
- `.planning/phases/01-structural-foundation-and-test-seam/01-VERIFICATION.md` — provider 边界与夹具纪律

### 受限方法与契约
- `tests/common/mod.rs` — `EXPECTED_METHODS`（本阶段增补 `GetProtocolVersion`）与夹具完备性测试
- `tests/fixtures/v0.2.0/README.md` — 契约变更必须显式记录、不得放宽 `normalize`
- `api/skf_api.js` — v0.2.0 客户端兼容基线
- `docs/OPERATION-CLASSIFICATION.md` — 只读/破坏性分类（受限集合独立于它）

### 现有实现
- `src/main.rs` — `env_logger::init()`、子命令分派、`provider_load_failed`（L-05）
- `src/win_service.rs` — `redirect_stdio_to_log`、`run_service`、`status`
- `src/service_state.rs` — 状态文件（需加 `started_at`）
- `src/protocol/mod.rs` / `params.rs` — `RpcRequest`/`RpcResponse`/`Language`
- `src/server/mod.rs` — `bind_with_progress`、`ServerOptions`
- `src/config/mod.rs` — `load`/`validate`/`resolve_lib_path`
- `packaging/windows/build.ps1` + `.github/workflows/release-windows.yml` — 发布产物与既有 `Get-PeMachine`
- `Cargo.toml` — 版本与依赖

**本项目无外部规格或 ADR 文档** —— 需求与约束完整记录于上述 `.planning/` 文件与源码本身。

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **`bind_with_progress`（阶段 5）**：诊断可直接复用它实现"config/provider/bind"检查，无需重复解析逻辑；也可用 `config::load`/`resolve_lib_path` 单独调用。
- **`service-state.json`（阶段 5）**：诊断读取它获得运行中服务的地址与（新增）`started_at`/uptime。
- **`Get-PeMachine`（`build.ps1`）**：release workflow 解包校验可直接复用其逻辑。
- **`tests/common::EXPECTED_METHODS`**：新增方法的契约增补点。
- **`classification.rs`**：受限集合的邻近落点，且已有不变量测试风格可借鉴。

### Established Patterns
- 平台逻辑与纯逻辑分离（阶段 1–5）：`logging`/`diagnostic`/`service_state`/`protocol` 都应在任意平台编译并可单测；Windows 专属仅 CLI 接线与 SCM。
- 契约纪律：新增方法必须显式声明为增补，不得放宽 `normalize`。
- 工具链固定 1.93.0，CI `-D warnings`，fmt 干净——新代码必须满足。
- 敏感信息按 AGENTS.md：不写日志、不写规划文档、不进发布元数据。

### Integration Points
- `src/main.rs` 的子命令分派（在 `#[cfg(windows)]` 之前）是 `diagnose` 的接线点。
- `src/logging.rs` 被 `main.rs`（前台）与 `win_service.rs`（服务）共同初始化。
- `src/protocol/mod.rs` 的 `RpcRequest` 是 `apiVersion` 字段落点；`main.rs` 的 `handle_request` 入口是版本校验落点。
- `packaging/windows/build.ps1` 与 `release-windows.yml` 是 checksum 与解包校验落点。
- `tracing` 无关；全部保留 `log`。

</code_context>

<specifics>
## Specific Ideas

- 用户接受 **`flexi_logger` + JSON Lines + 10 MiB×7 保留** 与**源码扫描脱敏测试**。
- 用户接受 **`diagnose` 子命令**（五阶段布尔 + 运行时状态 + uptime），且只输出非敏感事实；顺带修复 L-05。
- 用户接受 **可选 `apiVersion` + 新 `GetProtocolVersion`（文档化增补）**，以及 **`SKF_RESTRICT_LEGACY=1` 时受限方法返回 `-100`**、默认行为不变。
- 用户接受 **`.sha256` 文件 + workflow 解包校验 + `RELEASE-NOTES.md`**，并接受**版本升到 0.3.0**。
- 用户接受 **`THREAT-MODEL.md` + 中英对照 `docs/SESSION-AND-LIMITS.md` + 更新 agent 文档**。

</specifics>

<deferred>
## Deferred Ideas

- **TLS 与客户端认证** → 需要独立的传输安全设计；本里程碑仍本机优先。
- **HTTP demo 的非回环限制** → 阶段 3 已明确排除，威胁模型中作为已知边界记录。
- **`IssueCertificate` 的 OpenSSL 子进程无界** → 阶段 3 记录；本阶段在受限方法文档中说明。
- **结构化日志远程汇聚（syslog/OTLP）** → 本阶段只做本地文件轮转。
- **`GetProtocolVersion` 之外的能力协商（feature flags）** → 后续里程碑。
- **厂商 DLL 线程安全证据驱动的锁放宽** → 阶段 3 记录。
- **`service-state.json` 的版本化 schema** → 若诊断字段继续增长再引入。

</deferred>

---

*Phase: 06-observability-diagnostics-and-release-verification*
*Context gathered: 2026-09-11*

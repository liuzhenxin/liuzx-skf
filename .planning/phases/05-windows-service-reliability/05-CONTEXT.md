# Phase 5: Windows Service Reliability - Context

**Gathered:** 2026-09-11
**Status:** Ready for planning

<domain>
## Phase Boundary

让 Windows 服务如实反映自身可用性，并让安装、升级、卸载在权限与清理上可靠（SVC-01..05）。

具体交付：

1. **SVC-01**：SCM 在初始化期间看到 `StartPending`，只有在 WebSocket 监听器已绑定**且**配置的 provider 已解析后才看到 `Running`。
2. **SVC-02**：配置无效时以**不同的非零服务退出码**退出，使 `sc failure` 配置的重启动作真正触发。
3. **SVC-03**：安装器对安装目录施加限制性权限，非管理员无法替换已加载的 exe/DLL。
4. **SVC-04**：安装、运行、**在运行中的安装上覆盖升级**、卸载都不留下残留注册或锁定文件，且可重装。
5. **SVC-05**：`status` 通过报告失败/完成的启动阶段，区分"进程在跑"与"服务可用"。

**本阶段不做**：改变服务账户（保持 LocalSystem）；结构化日志/轮转（阶段 6）；发布校验与协议版本（阶段 6）；CI 门禁本身（阶段 4，已完成）。

**硬约束**：SVC 行为只能在 Windows 上真实验证；本机为 macOS，因此交付物必须包含**交叉编译检查 + 跨平台纯逻辑单测 + Windows 人工验证脚本**。37 个 v0.2.0 夹具必须继续匹配。

</domain>

<decisions>
## Implementation Decisions

### 启动阶段与 status 可见性（SVC-01 / SVC-05，区域 1）
- **D-01:** 服务在与 exe 同目录写一个**状态文件** `service-state.json`，逐阶段更新：
  `{"state":"StartPending","stage":"config","pid":N,"updated":ISO}` →
  `{"state":"StartPending","stage":"provider"}` →
  `{"state":"StartPending","stage":"bind"}` →
  `{"state":"Running","stage":"serve","address":"127.0.0.1:9001"}`；
  失败写 `{"state":"Failed","stage":"<stage>","code":N,"reason":"<short>"}`；
  正常停止写 `{"state":"Stopped"}`。
- **D-02:** 写入必须**原子**（写临时文件 + rename），避免 `status` 读到撕裂内容。文件位于 exe 同目录（`%ProgramFiles(x86)%\LiuZX\SKF Service\`），服务为 LocalSystem 可写；`status` 读取它。
- **D-03:** 状态文件**不得**包含任何配置值、库路径细节、PIN 或密钥材料；只含 `state`/`stage`/`code`/`reason`/`pid`/`address`/`updated`。
- **D-04:** `status` 优先读状态文件并把阶段打印出来；文件不存在时回退到现有 SCM-only 输出（保持向后兼容）。

### 失败退出码与重启（SVC-02，区域 2）
- **D-05:** 失败通过 `ServiceExitCode::ServiceSpecific(u32)` 上报，代码按失败类区分并文档化：
  `1` = 配置加载失败；`2` = provider 解析失败；`3` = WebSocket 绑定失败；`4` = serve 运行期错误。
- **D-06:** 失败时先写 `Failed` 状态文件（含 code 与短 reason），再上报 `Stopped` + 对应退出码；`run_service` 返回错误，`service_main` 记录日志。
- **D-07:** 重启动作继续由安装器的 `sc failure <name> reset= 86400 actions= restart/5000/restart/10000/restart/60000` 配置（`install.ps1` 已有）；Rust 侧只负责"以非零码结束"，不自行配置恢复动作。

### SVC-01 / SVC-02 的边界（**需注意的平台交互约束**）
- **D-08:** **"invalid configuration" = 致命**：YAML 不可解析、`default` 别名不存在、或当前 OS 没有该 provider 的路径配置。这些在 `bind` 之前判定，以非零码退出（`1` 或 `2`）。
- **D-09:** **"path 已配置但库文件缺失/加载失败" = 非致命**：继续降级为 `UnavailableProvider`，服务仍报 `Running`，各请求返回既有的 `Load Lib Failed` 错误。
  **理由**：Phase 3 的 CI 在 Linux 上故意没有厂商库，并依赖服务能够启动（`tests/session_isolation.rs` 等会 spawn 二进制）。若把库加载失败设为致命，Linux 的硬件无关测试会全部失败。此边界写入 `docs/` 并在 SUMMARY 说明。
- **D-10:** `Running` 在 `server::bind` 成功返回（监听器已绑定、配置已加载、provider 已按 D-08/D-09 解析）之后上报。实现用 `run_server` 已有的 `bound_tx` 通道观察"已绑定"，而不是固定 sleep。

### 安装目录权限（SVC-03，区域 3）
- **D-11:** `install.ps1` 在复制负载后对 `$InstallDir` 显式施加 ACL：
  `icacls "$InstallDir" /inheritance:r /grant:r "SYSTEM:(OI)(CI)F" "Administrators:(OI)(CI)F" "Users:(OI)(CI)RX"`
- **D-12:** 安装脚本在设置后**验证**：`icacls "$InstallDir"` 的输出中 `Users` 不得含 `(W)`、`(M)` 或 `(F)`；含则报错退出。
- **D-13:** 服务账户保持 **LocalSystem**（GM3000 驱动访问要求）；SVC-03 只约束文件 ACL，不改变账户。PROJECT.md 中"降低权限需验证驱动访问"的 blocker 因此不触发。

### 升级/卸载健壮性（SVC-04，区域 4）
- **D-14:** `install.ps1` / `uninstall.ps1` 用**轮询等待**替代 `Start-Sleep 800ms`：停止后轮询 SCM 状态为 `Stopped`，并尝试以独占方式打开 `skf-service.exe`（`[IO.File]::Open($Exe,'Open','ReadWrite','None')`）；两者都成功或超时（如 30s）才继续；超时则报错退出。
- **D-15:** 卸载同样等待进程退出与注册移除后再删目录；`Remove-Item` 失败不得静默（当前是 `SilentlyContinue`）。
- **D-16:** 新增 `packaging/windows/verify-service.ps1`：自动执行 install → status → **在运行中重跑 install.ps1 覆盖升级** → status → uninstall → 确认无注册残留与无锁定文件 → reinstall → uninstall，每步断言。用于 Windows 人工 UAT，也可作为将来 CI 的 Windows job 的雏形。

### 可测试性（跨平台）
- **D-17:** 纯逻辑抽到可在任意平台单测的模块：状态文件的序列化/解析与原子写、失败类→退出码映射、ACL 命令与校验的构造。CI 在 ubuntu/macOS 上运行这些单测。
- **D-18:** Windows 专属行为（SCM 上报顺序、真实重启、ACL 生效、升级锁）由 `verify-service.ps1` 在 Windows 上人工验证，并记录到 `05-HUMAN-UAT.md`。

### the agent's Discretion
- 状态文件字段的确切 schema 与状态枚举命名。
- 原子写的实现（`tempfile` 依赖 vs 手动 rename）；优先不新增依赖。
- `bind` 返回的启动错误如何被分类（新增错误枚举 vs 解析现有 anyhow 链）。
- PowerShell 脚本的辅助函数结构。
- 退出码常量在 Rust 与 PowerShell 两处的一致性维护方式（建议 Rust 定义、脚本注释引用）。

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### 阶段范围与验收标准
- `.planning/ROADMAP.md` §Phase 5 — 目标、依赖、5 条成功标准
- `.planning/REQUIREMENTS.md` §Windows Service Reliability — SVC-01..05
- `.planning/PROJECT.md` — Constraints（i686、LocalSystem/驱动访问 blocker）、Out of Scope

### 现有 Windows 实现
- `src/win_service.rs` — `install`/`uninstall`/`start`/`stop`/`status`/`run`/`run_service`/`delete_if_present`/`wait_until_removed`；当前上报顺序与退出码问题所在
- `src/main.rs` — `main()` 中的 Windows SCM 分派（`install`/`uninstall`/`--service` 等）
- `src/server/mod.rs` — `bind`（配置加载→provider 构建→监听器绑定）、`serve`、`run_server` 的 `bound_tx: Option<oneshot::Sender<BoundServer>>` 通道、`NativeProviderFactory`/`UnavailableProvider`
- `src/config/mod.rs` — `load`/`validate`（判定配置无效的现有逻辑）

### 打包与安装
- `packaging/windows/install.ps1` / `uninstall.ps1` / `install.bat` / `uninstall.bat` — 现有安装流程、`sc failure` 配置、`Start-Sleep` 等待
- `packaging/windows/build.ps1` — 产出 ZIP、`Machine=0x014C` 断言、负载布局（`config/`、`native/GM3000/windows/`、`api/`）
- `packaging/windows/README-Windows-x86_64.md` — 面向用户的安装说明
- `.github/workflows/release-windows.yml` — 既有 Windows 构建作业

### 前序阶段约束
- `.planning/phases/03-transport-hardening-and-concurrency/03-VERIFICATION.md` — Linux CI 依赖服务在无厂商库时可启动（D-09 的理由）
- `.planning/phases/01-structural-foundation-and-test-seam/01-VERIFICATION.md` — `bound_tx` 与临时端口测试的由来
- `.planning/phases/04-format-lint-normalization-and-ci-gate/04-VERIFICATION.md` — 现有 CI job 与工具链约束

**本项目无外部规格或 ADR 文档** —— 需求与约束完整记录于上述 `.planning/` 文件与源码本身。

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **`bound_tx` 通道**（`run_server` 的第四个参数）：`bind` 成功后立刻发送 `BoundServer`。`run_service` 可用它确定"已绑定"再上报 Running，无需猜测时序。
- **`wait_until_removed`**（`win_service.rs`）：轮询 SCM 直到服务消失；可扩展为"等进程退出 + 文件解锁"。
- **`delete_if_present`**：幂等安装已有雏形；升级时复用并加强等待。
- **`sc failure` 配置**（`install.ps1`）：SVC-02 的恢复动作已存在，只需让服务以非零码失败。
- **`ServerOptions::from_env(RunMode::Service)`**：服务模式的 HTTP 默认已是回环。

### Established Patterns
- 启动用 `anyhow` + context；请求边界用 `RpcResponse::err`。
- Windows 专属代码 `#[cfg(windows)]` 在模块边界；`win_service` 仅在 Windows 编译。
- 纯逻辑与平台逻辑分离（阶段 1–4 的 crypto/config 先例），使 macOS/Linux CI 能覆盖大部分逻辑。
- 契约纪律：任何响应漂移必须显式声明。

### Integration Points
- `src/win_service.rs::run_service` 是上报顺序与退出码的唯一落点。
- `src/server/mod.rs::bind` 是"配置/provider/绑定"三阶段错误分类的落点（可能新增错误枚举）。
- `packaging/windows/install.ps1` / `uninstall.ps1` 是 ACL 与等待逻辑的落点。
- 状态文件的读写分别落在 `win_service.rs`（写）与 `status`（读），纯逻辑可抽到独立模块供单测。

</code_context>

<specifics>
## Specific Ideas

- 用户明确接受**状态文件 + 原子写**作为 `status` 反映阶段的手段。
- 用户接受**分类退出码**（1 配置 / 2 provider / 3 绑定 / 4 serve）并保持 `sc failure` 配置不变。
- 用户接受**显式 icacls**（SYSTEM:F / Administrators:F / Users:RX）并在安装时校验。
- 用户接受**轮询等待进程退出与文件解锁**替代固定 sleep，并新增 Windows 人工验证脚本。
- 用户接受为保住 Linux CI 而区分的 D-08/D-09 边界（配置无效致命、库加载失败降级）。

</specifics>

<deferred>
## Deferred Ideas

- **服务账户最小权限化** → 保持 LocalSystem；若将来降低权限，需先验证 GM3000 驱动访问（PROJECT.md blocker）。
- **结构化日志、轮转、脱敏诊断** → 阶段 6。
- **发布校验、协议版本协商、迁移说明** → 阶段 6。
- **在 CI 中运行 Windows 服务实机测试** → 本阶段只提供 `verify-service.ps1`；托管 CI 的 Windows 实机作业留待后续评估。
- **`service-state.json` 的版本化 schema** → 若阶段 6 的诊断需要更多字段，再引入版本号。

</deferred>

---

*Phase: 05-windows-service-reliability*
*Context gathered: 2026-09-11*

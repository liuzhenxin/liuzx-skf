# Phase 5: Windows Service Reliability - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-09-11
**Phase:** 05-windows-service-reliability
**Areas discussed:** 启动阶段/status, 失败退出码与重启, 安装目录权限, 升级/卸载健壮性
**Mode:** 用户选择 "全部采用推荐"

---

## 1. 启动阶段与 status 可见性（SVC-01 / SVC-05）

| Option | Description | Selected |
|--------|-------------|----------|
| **状态文件 + 原子写** | exe 同目录 `service-state.json`，逐阶段更新；`status` 读取并打印阶段 | ✓ |
| 只用 SCM checkpoint/wait_hint | 无法给出阶段名 | |
| 命名管道/事件 | 更复杂，收益不明显 | |

**User's choice:** 1a
**Notes:** 文件须原子写、不得含敏感值；`status` 在文件缺失时回退 SCM-only。

---

## 2. 失败退出码与重启（SVC-02）

| Option | Description | Selected |
|--------|-------------|----------|
| **分类退出码** | `1` 配置 / `2` provider / `3` 绑定 / `4` serve；失败写状态文件 | ✓ |
| 单一非零码 | 可触发重启但无法区分 | |

**User's choice:** 2a
**Notes:** 重启动作保持由 `install.ps1` 的 `sc failure` 配置，Rust 侧只管非零退出。

---

## 3. 安装目录权限（SVC-03）

| Option | Description | Selected |
|--------|-------------|----------|
| **install.ps1 显式 icacls + 校验** | SYSTEM:F / Administrators:F / Users:RX；校验 Users 无写权限 | ✓ |
| 依赖 Program Files 继承 | 多数场景可行，但非"installer applies" | |
| Rust install 里用 Windows API | 实现更重 | |

**User's choice:** 3a
**Notes:** 服务账户保持 LocalSystem；SVC-03 只约束文件 ACL。

---

## 4. 升级/卸载健壮性（SVC-04）

| Option | Description | Selected |
|--------|-------------|----------|
| **轮询等待退出与解锁 + 人工验证脚本** | 停止后轮询 SCM 状态与独占打开 exe，超时报错；新增 `verify-service.ps1` | ✓ |
| 维持 Start-Sleep + 文档要求手动 stop | 不满足"覆盖升级"目标 | |

**User's choice:** 4a
**Notes:** 卸载等待进程退出与注册移除，`Remove-Item` 失败不再静默。

---

## 额外澄清（写 CONTEXT 前主动提出）

**D-08/D-09 边界**：SVC-01/02 的"provider resolved / invalid configuration"如何界定？
- 结论：**配置无效**（YAML 不可解析、default 别名缺失、当前 OS 无路径）→ 致命非零退出；**路径已配置但库缺失/加载失败** → 降级 `UnavailableProvider` 并报 Running。
- 理由：Phase 3 的 Linux CI 故意无厂商库且依赖服务可启动。若把加载失败设为致命，硬件无关测试会全部失败。
- 用户对该边界未提出异议（在 "全部采用推荐" 之后）。

---

## the agent's Discretion

- 状态文件 schema 与枚举命名；原子写实现（优先不新增依赖）。
- `bind` 启动错误的分类方式（新错误枚举 vs 解析 anyhow 链）。
- PowerShell 辅助函数结构；退出码常量在两处的一致性维护。

## Deferred Ideas

- 服务账户最小权限化（需先验证驱动访问）。
- 结构化日志/轮转/脱敏诊断（阶段 6）。
- 发布校验、协议版本、迁移说明（阶段 6）。
- 托管 CI 中的 Windows 实机服务测试。
- `service-state.json` 的版本化 schema。

# Phase 6: Observability, Diagnostics, and Release Verification - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-09-11
**Phase:** 06-observability-diagnostics-and-release-verification
**Areas discussed:** 日志, 诊断, 协议版本与受限方法, 发布完整性与文档
**Mode:** 用户选择 "全部采用推荐"

---

## 1. 日志（OBS-01 / OBS-02）

| Option | Description | Selected |
|--------|-------------|----------|
| **`flexi_logger` + JSON Lines + 10 MiB×7 + 脱敏扫描** | 单依赖，保留 `log` facade，改动最小 | ✓ |
| 迁移 `tracing` + `tracing-appender` | 更标准但改动面大 | |
| 仅轮转不结构化 | 不满足 "structured" | |

**User's choice:** 1a（含 1a-i 运行时 `sanitize()` + 源码扫描测试）
**Notes:** 服务的 stdout 重定向改为独立 `skf-service.console.log`，避免与结构化日志混写。

---

## 2. 诊断（OBS-03 / OBS-04）

| Option | Description | Selected |
|--------|-------------|----------|
| **`skf-service.exe diagnose`（五阶段布尔 + 运行时状态 + uptime）** | 跨平台、可脚本化、可 `--json` | ✓ |
| 仅读 `service-state.json` | 服务未运行时无信息 | |
| HTTP/WS 诊断端点 | 扩大攻击面，需认证设计 | |

**User's choice:** 2a
**Notes:** 只输出非敏感事实；顺带修复阶段 2 的 L-05（错误消息暴露库路径）。`service-state.json` 新增 `started_at` 以计算 uptime。

---

## 3. 协议版本与受限方法（REL-01 / REL-02）

| Option | Description | Selected |
|--------|-------------|----------|
| **可选 `apiVersion` + 新 `GetProtocolVersion`（文档化增补）+ `SKF_RESTRICT_LEGACY=1` 时受限方法返回 `-100`** | 旧客户端不变，默认行为不变 | ✓ |
| 仅文档化受限方法，不引入运行时 gate | 无法演示"返回文档化错误" | |
| 默认即拒绝受限方法 | 破坏 `IssueCertificate` 夹具 | |

**User's choice:** 3a
**Notes:** `EXPECTED_METHODS` 增补 `GetProtocolVersion` 并引入 `ADDITIVE_METHODS` 白名单；受限集合 `{IssueCertificate, Transmit}`；版本升 `0.3.0`。

---

## 4. 发布完整性与文档（REL-03/04/05 + DOC-01/02/03）

| Option | Description | Selected |
|--------|-------------|----------|
| **`.sha256` 文件 + workflow 解包校验 + `RELEASE-NOTES.md` + 威胁模型 + 中英对照文档** | 完整满足 5 条成功标准 | ✓ |
| checksum 只在 release 附件；中英独立文件 | 校验弱化、维护成本高 | |

**User's choice:** 4a
**Notes:** `Cargo.toml` 升至 `0.3.0`；`GetProtocolVersion.service` 取 `CARGO_PKG_VERSION`。

---

## the agent's Discretion

- `flexi_logger` 版本与 API 形态（实现时以 registry 源码为准）。
- JSON Lines 字段名与转义实现。
- `diagnose` 文本/`--json` 排版。
- `ADDITIVE_METHODS` 的组织方式。
- 两份新文档的章节顺序。

## Deferred Ideas

- TLS 与客户端认证。
- HTTP demo 的非回环限制。
- `IssueCertificate` OpenSSL 子进程无界。
- 结构化日志远程汇聚（syslog/OTLP）。
- 能力协商（feature flags）。
- 厂商 DLL 线程安全证据驱动的锁放宽。
- `service-state.json` 的版本化 schema。

# Phase 7: TLS Termination - Discussion Log

> **Audit trail only.** Decisions are captured in CONTEXT.md.

**Date:** 2026-09-12
**Phase:** 07-tls-termination
**Areas discussed:** 配置形态, TLS 默认值与 client_auth, 错误归属与可观测, 测试证书策略
**Mode:** 用户选择 "默认"（全部采用推荐）

---

## 1. 配置形态与路径解析

| Option | Description | Selected |
|--------|-------------|----------|
| **YAML 具名 `tls:` + 环境变量覆盖** | 路径相对 exe；与 config/api 一致 | ✓ |
| 仅环境变量 | 不便随安装分发 | |
| 内联 PEM | 私钥写入 YAML，违反密钥卫生 | |

**User's choice:** 1a

## 2. TLS 默认值与 `client_auth`

| Option | Description | Selected |
|--------|-------------|----------|
| **默认关闭；ring；min TLS1.2；只接受 client_auth=none，其它启动报错** | 不静默降级 | ✓ |
| 本期不引入 client_auth 字段 | 阶段 8 再加 | |
| 允许 TLS1.0/1.1 | 不安全 | |

**User's choice:** 2a

## 3. 错误归属与可观测

| Option | Description | Selected |
|--------|-------------|----------|
| **归入 StartupError::Config（exit 1）；消息仅类别；diagnose 加非敏感布尔** | | ✓ |
| 新增 StartupError::Tls（exit 5） | | |
| 只在阶段 10 文档提及 | | |

**User's choice:** 3a

## 4. 测试证书策略

| Option | Description | Selected |
|--------|-------------|----------|
| **rcgen dev-dependency（hermetic）** | 无 OpenSSL 文本依赖 | ✓ |
| 签入固定测试证书 | 私钥入库、过期风险 | |
| openssl CLI | 版本漂移（IssueCertificate 教训） | |

**User's choice:** 4a

## the agent's Discretion

- `TlsConfig` 类型/校验；泛型 vs 装箱流；`rcgen` 版本与测试辅助组织；启动日志措辞。

## Deferred Ideas

- mTLS / bearer 令牌（阶段 8）
- 非回环绑定收紧（阶段 9）
- 授权审计（阶段 9）
- HTTP demo 的 TLS（未排期）

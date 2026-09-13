---
phase: 7
slug: tls-termination
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-09-12
---

# Phase 7 — Validation Strategy

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test`；TLS 客户端用 `tokio-rustls`；测试证书用 `rcgen`（dev-dependency） |
| **Quick run command** | `cargo test --lib tls` |
| **Full suite command** | `cargo test` + `cargo check --all-targets` + `cargo check --target i686-pc-windows-gnu` |
| **Estimated runtime** | ~30 s |

## Sampling Rate

- **After every task commit:** `cargo test --lib`
- **After every plan wave:** `cargo test` + `cargo check --target i686-pc-windows-gnu`
- **Before `$gsd-verify-work`:** full suite green；`contract_fixtures` green
- **Max feedback latency:** ~30 s

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 07-01-01 | 07-01 | 1 | TLS-01, TLS-03 | T-07-01 | `tls:` 具名解析、env 覆盖、缺文件即错 | unit | `cargo test tls` | ❌ W1 | ⬜ pending |
| 07-01-02 | 07-01 | 1 | TLS-01 | T-07-02 | 证书/私钥加载失败 → `StartupError::Config`（exit 1），消息仅类别 | unit | `cargo test server::` | ❌ W1 | ⬜ pending |
| 07-01-03 | 07-01 | 1 | TLS-02, TLS-03, COMPAT-01 | T-07-03 | 泛型连接循环；TLS 可选；明文默认不变 | integration | `cargo test --test tls` | ❌ W1 | ⬜ pending |
| 07-01-04 | 07-01 | 1 | TLS-02 | T-07-03 | hermetic 证书下 TLS 客户端握手并调用方法 | integration | `cargo test --test tls` | ❌ W1 | ⬜ pending |
| 07-02-01 | 07-02 | 2 | TLS-04 | T-07-04 | `diagnose` 只报 `tls_enabled`/`tls_cert_loaded` 布尔 | unit | `cargo test diagnostic` | ✅ W2 | ⬜ pending |
| 07-02-02 | 07-02 | 2 | TLS-04 | T-07-04 | 日志/诊断/状态文件不含私钥路径或内容；脱敏扫描通过 | integration/scan | `cargo test --test tls --test log_redaction` | ❌ W2 | ⬜ pending |
| 07-02-03 | 07-02 | 2 | COMPAT-01 | T-07-05 | 37 夹具在默认明文下保持匹配；i686 交叉编译通过 | integration | `cargo test --test contract_fixtures && cargo check --target i686-pc-windows-gnu` | ✅ W2 | ⬜ pending |

## Wave 0 Requirements

- [x] `src/server/mod.rs` / `src/config/mod.rs` 现有结构（阶段 3/5）
- [x] `src/diagnostic.rs` / `src/logging.rs`（阶段 6）
- [ ] `rcgen` dev-dependency（07-01-04 加入）
- [ ] `tests/tls.rs`（07-01-04 创建）

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| 真实证书（非自签）的 TLS 握手 | TLS-02 | 需要运维提供的证书链 | 在测试机放置真实证书/私钥，启动服务，用真实 CA 信任的客户端连接并调用 `SetLanguage` |
| Windows 服务模式下的 TLS | TLS-02 | 服务模式实机 | 在 Windows 上用 `tls:` 配置启动服务，`diagnose` 显示 `tls_enabled=true`，客户端可经 TLS 调用 |

## Validation Sign-Off

- [ ] 每个任务具备自动化命令
- [ ] 采样连续性：不存在连续 3 个任务缺少自动化验证
- [ ] Wave 0 覆盖全部缺失引用
- [ ] 无 watch-mode 标志
- [ ] 反馈延迟 < 30 s
- [ ] `nyquist_compliant: true`

**Approval:** pending

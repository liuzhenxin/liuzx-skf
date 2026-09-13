---
phase: 9
slug: bind-policy-and-authorization-audit
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-09-12
---

# Phase 9 — Validation Strategy

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test`；绑定用例用 `rcgen` 生成 TLS 材料；审计用真实二进制的 stderr 捕获（`RUST_LOG=info`） |
| **Quick run command** | `cargo test --lib` |
| **Full suite command** | `cargo test` + `cargo check --target i686-pc-windows-gnu` |
| **Estimated runtime** | ~30 s |

## Sampling Rate

- **After every task commit:** `cargo test --lib`
- **After every plan wave:** `cargo test` + `cargo check --target i686-pc-windows-gnu`
- **Before `$gsd-verify-work`:** full suite green (except the hardware-blocked contract replay)
- **Max feedback latency:** ~30 s

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|--------|
| 09-01-01 | 09-01 | 1 | AUD-01, AUD-02 | T-09-01 | `AuditEvent` JSON 只含非敏感字段 | unit | `cargo test audit` | ⬜ pending |
| 09-01-02 | 09-01 | 1 | AUD-01 | T-09-02 | `SessionState` 的 grant/authorize/invalidate 发出审计 | unit+scan | `cargo test session` | ⬜ pending |
| 09-02-01 | 09-02 | 1 | BIND-01, BIND-02 | T-09-03 | 非回环需 opt-in+TLS+客户端认证，绑定前拒绝、exit 3 | unit | `cargo test server::` | ⬜ pending |
| 09-02-02 | 09-02 | 1 | BIND-03 | T-09-04 | 回环默认不变；loopback_gate 更新（正值带 TLS；新增缺 TLS 负值） | integration | `cargo test --test loopback_gate` | ⬜ pending |
| 09-03-01 | 09-03 | 2 | AUD-01, AUD-02 | T-09-02 | 真实服务产生 `authorization.denied` 审计行且不含密钥 | integration | `cargo test --test audit` | ⬜ pending |
| 09-03-02 | 09-03 | 2 | AUD-02, BIND | T-09-01 | 脱敏扫描含审计词；fmt/clippy/i686 回归 | scan | `cargo test --test log_redaction` | ⬜ pending |

## Wave 0 Requirements

- [x] `SessionState::{grant,authorize,invalidate_device}` 是唯一决策点
- [x] `logging.rs` 的 console/service 日志已就绪
- [ ] `src/audit.rs`（09-01-01 创建）
- [ ] `tests/audit.rs`（09-03-01 创建）

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| 真实设备移除触发 `device.unavailable` | AUD-01 | 需要硬件 | 在 Windows 上运行 UAT B2，检查日志出现 `device.unavailable` 且 `cleared>0` |
| 真实证书/令牌下的非回环绑定 | BIND-01 | 部署环境 | 配置 opt-in + 真实 TLS + mtls/token，确认可绑定并可连接 |

## Validation Sign-Off

- [ ] 每个任务具备自动化命令
- [ ] 采样连续性：不存在连续 3 个任务缺少自动化验证
- [ ] Wave 0 覆盖全部缺失引用
- [ ] 无 watch-mode 标志
- [ ] 反馈延迟 < 30 s
- [ ] `nyquist_compliant: true`

**Approval:** pending

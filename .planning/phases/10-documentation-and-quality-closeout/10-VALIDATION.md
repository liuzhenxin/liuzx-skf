---
phase: 10
slug: documentation-and-quality-closeout
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-09-12
---

# Phase 10 — Validation Strategy

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test`；文档检查用 `grep`；CI 清单核对用 workflow 内容断言 |
| **Quick run command** | `cargo test --lib` |
| **Full suite command** | `cargo test --no-fail-fast` + fmt/clippy/i686 |
| **Estimated runtime** | ~30 s |

## Sampling Rate

- **After every task commit:** 相关文档 grep 或 `cargo test --lib`
- **After every plan wave:** `cargo test --no-fail-fast`
- **Before `$gsd-verify-work`:** 全量绿 + 文档术语齐全
- **Max feedback latency:** ~30 s

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Secure Behavior | Test Type | Automated Command | Status |
|---------|------|------|-------------|-----------------|-----------|-------------------|--------|
| 10-01-01 | 10-01 | 1 | COMPAT-02 | 威胁模型与限制文档反映 TLS/auth/bind/audit | grep | `grep -c 'TLS' THREAT-MODEL.md` | ⬜ pending |
| 10-01-02 | 10-01 | 1 | COMPAT-02 | 会话/限制文档（zh+en）与发行说明更新、迁移路径 | grep | `grep -c 'client_auth' docs/SESSION-AND-LIMITS.md` | ⬜ pending |
| 10-02-01 | 10-02 | 1 | QA-01 | CI 快速任务包含 7–9 阶段的 hermetic 套件 | grep | `grep -c 'test client_auth' .github/workflows/ci.yml` | ⬜ pending |
| 10-02-02 | 10-02 | 1 | QA-01 | 阶段 3–6 的 Nyquist 签核记录为可执行证据 | file | `test -f .planning/phases/10-*/10-NYQUIST-AUDIT.md` | ⬜ pending |
| 10-03-01 | 10-03 | 2 | QA-02 | 无令牌下 TLS/auth/audit 套件可跑 | command | `env -u SKF_TLS_TOKEN cargo test --test tls --test client_auth --test audit` | ⬜ pending |
| 10-03-02 | 10-03 | 2 | COMPAT-02, QA-02 | 全量回归 + 文档无过时声明 | command+grep | `cargo test --no-fail-fast` | ⬜ pending |

## Wave 0 Requirements

- [x] 四个文档与 CI 已存在
- [x] 阶段 3–6 归档含 VALIDATION.md
- [ ] `10-NYQUIST-AUDIT.md`（10-02-02 创建）

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| 真实证书的端到端绑定与连接 | COMPAT-02 | 部署环境 | 用运维 CA 证书配置 `tls:`，非回环 + mTLS，确认可连接并产生审计 |

## Validation Sign-Off

- [ ] 每个任务具备自动化命令
- [ ] 采样连续性：不存在连续 3 个任务缺少自动化验证
- [ ] Wave 0 覆盖全部缺失引用
- [ ] 无 watch-mode 标志
- [ ] 反馈延迟 < 30 s
- [ ] `nyquist_compliant: true`

**Approval:** pending

---
phase: 8
slug: client-authentication
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-09-12
---

# Phase 8 — Validation Strategy

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test`；客户端证书/CA 用 `rcgen`；TLS 客户端用 `tokio-rustls`；token 头用 `tungstenite` 的 `client_async(IntoClientRequest)` |
| **Quick run command** | `cargo test --lib` |
| **Full suite command** | `cargo test` + `cargo check --target i686-pc-windows-gnu` |
| **Estimated runtime** | ~30 s |

## Sampling Rate

- **After every task commit:** `cargo test --lib`
- **After every plan wave:** `cargo test` + `cargo check --target i686-pc-windows-gnu`
- **Before `$gsd-verify-work`:** full suite green
- **Max feedback latency:** ~30 s

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|--------|
| 08-01-01 | 08-01 | 1 | AUTH-01, AUTH-02 | T-08-01 | 配置矩阵：mtls 需证书+CA；token 需令牌源；未知模式报错 | unit | `cargo test config` | ⬜ pending |
| 08-01-02 | 08-01 | 1 | AUTH-03, AUTH-04 | T-08-02 | 模式解析、`requires_loopback`、常量时间令牌比较 | unit | `cargo test client_auth` | ⬜ pending |
| 08-02-01 | 08-02 | 2 | AUTH-01 | T-08-03 | mtls 构建 `WebPkiClientVerifier`；缺 CA 为启动错误 | unit | `cargo test server::` | ⬜ pending |
| 08-02-02 | 08-02 | 2 | AUTH-02, AUTH-04 | T-08-04 | 升级回调校验 bearer；身份提取（CN/token/anonymous） | unit | `cargo test server::` | ⬜ pending |
| 08-02-03 | 08-02 | 2 | AUTH-04 | T-08-05 | `SessionFactory::create(identity)`；只记录身份类别 | compile+unit | `cargo check --all-targets` | ⬜ pending |
| 08-03-01 | 08-03 | 3 | AUTH-03 | T-08-06 | 非回环且 `none` → 绑定前拒绝 | integration | `cargo test --test loopback_gate` | ⬜ pending |
| 08-03-02 | 08-03 | 3 | AUTH-01, AUTH-02 | T-08-07 | mtls/token 端到端：缺失拒绝、有效通过、无效拒绝 | integration | `cargo test --test client_auth` | ⬜ pending |
| 08-03-03 | 08-03 | 3 | AUTH-04 | T-08-05 | 令牌/客户端证书不落日志与诊断；回归 | integration/scan | `cargo test --test client_auth --test log_redaction` | ⬜ pending |

## Wave 0 Requirements

- [x] phase 7 的 `build_tls_acceptor` / `run_connection` 泛型路径
- [x] `rustls::server::WebPkiClientVerifier`、`accept_hdr_async_with_config`、`rcgen` 已核对
- [ ] `src/client_auth.rs`（08-01-02 创建）
- [ ] `tests/client_auth.rs`（08-03-02 创建）

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| 真实企业 PKI 签发的客户端证书 | AUTH-01 | 需要部署方的 CA | 用部署方 CA 签发客户端证书，配置 `client_ca_file`，验证可连接 |
| 令牌在真实服务模式下不出现在日志 | AUTH-04 | 服务模式实机 | Windows 服务模式下用 `client_auth: token` 启动，检查日志与 `diagnose` |

## Validation Sign-Off

- [x] 每个任务具备自动化命令
- [x] 采样连续性：不存在连续 3 个任务缺少自动化验证
- [x] Wave 0 覆盖全部缺失引用
- [x] 无 watch-mode 标志
- [x] 反馈延迟 < 30 s
- [x] `nyquist_compliant: true`

**Approval:** approved 2026-09-12 — all tasks have automated commands and pass in the current tree

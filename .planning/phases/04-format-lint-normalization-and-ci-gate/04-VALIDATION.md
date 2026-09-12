---
phase: 4
slug: format-lint-normalization-and-ci-gate
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-09-11
---

# Phase 4 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo fmt` / `cargo clippy` / `cargo test`（Rust 内置） |
| **Config file** | `rust-toolchain.toml`（本阶段新增） |
| **Quick run command** | `cargo fmt --all -- --check` |
| **Full suite command** | `cargo test && cargo clippy --all-targets -- -D warnings` |
| **Estimated runtime** | ~20 s |

---

## Sampling Rate

- **After every task commit:** `cargo fmt --all -- --check`
- **After every plan wave:** `cargo test` + `cargo clippy --all-targets -- -D warnings`
- **Before `$gsd-verify-work`:** full suite green；`cargo test --test contract_fixtures` green
- **Max feedback latency:** ~15 s

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 04-01-01 | 04-01 | 1 | QUAL-01 | — | 全仓储格式化，仅格式变更 | command | `cargo fmt --all -- --check` exit 0 | ✅ | ⬜ pending |
| 04-01-02 | 04-01 | 1 | QUAL-01 | — | 格式化无语义变更 | regression | `cargo test --test contract_fixtures` | ✅ | ⬜ pending |
| 04-02-01 | 04-02 | 2 | QUAL-02 | — | 工具链固定，clippy 基线可复现 | structural | `grep -c 'channel = "1.93.0"' rust-toolchain.toml` == 1 | ❌ W2 | ⬜ pending |
| 04-02-02 | 04-02 | 2 | QUAL-02 | — | 机械类 lint 清零 | command | `cargo clippy --all-targets -- -D warnings` | ✅ | ⬜ pending |
| 04-02-03 | 04-02 | 2 | QUAL-02 | T-04-02 | `await_holding_lock` 修复；scoped allow 有注释 | command/structural | `cargo clippy --all-targets -- -D warnings` exit 0 | ✅ | ⬜ pending |
| 04-02-04 | 04-02 | 2 | QUAL-02 | — | clippy 硬失败证明 | command | `cargo clippy --all-targets -- -D warnings` exit 0 | ✅ | ⬜ pending |
| 04-03-01 | 04-03 | 3 | QUAL-03 | T-04-01 | CI 触发 PR 与 main/dev | structural | YAML 含 `pull_request` 与 `branches: [main, dev]` | ❌ W3 | ⬜ pending |
| 04-03-02 | 04-03 | 3 | QUAL-03 | T-04-03 | 契约重放在 x86_64 macOS | structural | `test-full` job 使用 x86_64 macOS runner | ❌ W3 | ⬜ pending |
| 04-03-03 | 04-03 | 3 | QUAL-04 | T-04-04 | 门禁对失败测试敏感 | command | 本地 dry-run `gate-selftest` 逻辑 | ❌ W3 | ⬜ pending |
| 04-03-04 | 04-03 | 3 | QUAL-03 | — | 所有 job 命令本地可跑 | command | fmt/clippy/test/i686 全部退出 0 | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] `cargo` 工具链可用（1.93.0）
- [x] `i686-pc-windows-gnu` target 已安装
- [ ] `rust-toolchain.toml`（04-02-01 创建）
- [ ] `.github/workflows/ci.yml`（04-03-01 创建）

*无新增测试框架；CI 的验证通过本地运行相同命令完成。*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| GitHub 分支保护确实阻断合并 | QUAL-04 | 仓库文件无法配置仓库设置 | 在 GitHub 设置中对 `main` 启用 "Require status checks to pass"，勾选本工作流 job；清单见 `docs/CI.md` |
| x86_64 macOS runner 标签可用 | QUAL-03 | 组织/账户相关，无法在本仓库验证 | 首次 workflow 运行确认；若标签不可用，按 RESEARCH §3.1 的 fallback 处理 |
| CI 在 PR 上真实触发 | QUAL-03 | 需要 push 到远端 | 开一个 draft PR，确认 6 个 job 出现且 `test-full` 加载厂商库 |

---

## Validation Sign-Off

- [ ] 每个任务具备自动化验证命令
- [ ] 采样连续性：不存在连续 3 个任务缺少自动化验证
- [ ] Wave 0 覆盖全部缺失引用
- [ ] 无 watch-mode 标志
- [ ] 反馈延迟 < 15 s
- [ ] 计划完成后置 `nyquist_compliant: true`

**Approval:** pending

---

*Phase: 04-format-lint-normalization-and-ci-gate*
*Validation strategy created: 2026-09-11*

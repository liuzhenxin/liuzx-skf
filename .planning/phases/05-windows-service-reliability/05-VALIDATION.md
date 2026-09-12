---
phase: 5
slug: windows-service-reliability
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-09-11
---

# Phase 5 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `cargo test` for cross-platform logic；PowerShell for the Windows runtime script |
| **Config file** | none（复用 `cargo`；新增 `packaging/windows/verify-service.ps1`） |
| **Quick run command** | `cargo test --lib` |
| **Full suite command** | `cargo test` + `cargo check --target i686-pc-windows-gnu` |
| **Estimated runtime** | ~20 s（不含 Windows 实机） |

---

## Sampling Rate

- **After every task commit:** `cargo test --lib`
- **After every plan wave:** `cargo test` + `cargo check --target i686-pc-windows-gnu`
- **Before `$gsd-verify-work`:** full suite green；`contract_fixtures` green
- **Max feedback latency:** ~20 s

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 05-01-01 | 05-01 | 1 | SVC-05 | — | 状态文件往返与原子写；不含敏感值 | unit | `cargo test service_state` | ❌ W1 | ⬜ pending |
| 05-01-02 | 05-01 | 1 | SVC-02 | — | 阶段→退出码映射 | unit | `cargo test service_state` | ❌ W1 | ⬜ pending |
| 05-01-03 | 05-01 | 1 | SVC-01 | — | bind 进度回调与 StartupError 分类 | unit | `cargo test server::` | ❌ W1 | ⬜ pending |
| 05-01-04 | 05-01 | 1 | SVC-01/02/05 | — | run_service 顺序、非零退出、status 阶段 | structural + Windows UAT | `cargo check --target i686-pc-windows-gnu` | ✅ | ⬜ pending |
| 05-02-01 | 05-02 | 2 | SVC-03 | T-05-01 | 安装目录 ACL 明确且校验 | structural + Windows UAT | `grep -c 'icacls' install.ps1` | ✅ | ⬜ pending |
| 05-02-02 | 05-02 | 2 | SVC-04 | T-05-02 | 升级/卸载等待退出与解锁 | structural + Windows UAT | `grep -c 'Start-Sleep' install.ps1` == 0 | ✅ | ⬜ pending |
| 05-02-03 | 05-02 | 2 | SVC-04 | T-05-02 | 人工验证脚本覆盖 install/upgrade/uninstall/reinstall | script exists | `test -f packaging/windows/verify-service.ps1` | ❌ W2 | ⬜ pending |
| 05-02-04 | 05-02 | 2 | SVC-01..05 | — | 文档与退出码一致 | docs | `grep -c 'EXIT_PER_CONFIG\|exit code' docs/WINDOWS-SERVICE.md` | ❌ W2 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] Rust 工具链与 `i686-pc-windows-gnu` target（阶段 4 已装）
- [x] `windows-service 0.7` 依赖已存在
- [ ] `src/service_state.rs`（05-01-01 创建）
- [ ] `packaging/windows/verify-service.ps1`（05-02-03 创建）

*无新增框架；Windows 运行时验证通过脚本进行。*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| StartPending → Running 顺序 | SVC-01 | 需要真实 SCM | 在 Windows VM 运行 `verify-service.ps1`；同时 `sc query` 观察启动瞬间状态 |
| 无效配置触发重启动作 | SVC-02 | 需要 SCM 恢复策略 | 放入坏 YAML，启动服务，确认 `sc query` 的退出码非零且 `sc failure` 动作触发（事件日志） |
| ACL 阻止非管理员替换 exe/DLL | SVC-03 | 需要真实账户与 ACL | 以标准用户尝试覆盖 `%ProgramFiles(x86)%\LiuZX\SKF Service\skf-service.exe`，必须被拒 |
| 运行中覆盖升级 | SVC-04 | 需要真实文件锁 | `verify-service.ps1` 的升级阶段；确认无残留、无锁定 |
| 重装成功 | SVC-04 | 同上 | `verify-service.ps1` 末尾 reinstall |
| `status` 区分进程与可用 | SVC-05 | 需要真实服务 | 运行中故意绑定失败（占用 9001 端口），`status` 必须显示 `Failed(bind)` |

---

## Validation Sign-Off

- [ ] 每个任务具备自动化命令或 Windows UAT 指令
- [ ] 采样连续性：不存在连续 3 个任务缺少自动化验证
- [ ] Wave 0 覆盖全部缺失引用
- [ ] 无 watch-mode 标志
- [ ] 反馈延迟 < 20 s
- [ ] 计划完成后置 `nyquist_compliant: true`

**Approval:** pending

---

*Phase: 05-windows-service-reliability*
*Validation strategy created: 2026-09-11*

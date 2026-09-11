---
phase: 2
slug: session-authorization-and-resource-ownership
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-09-11
---

# Phase 2 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust 内置 `cargo test`（`#[test]` / `#[tokio::test]`）；复用 Phase 1 的 lib 目标 |
| **Config file** | none — 无新测试框架；共享辅助继续放在 `tests/common/mod.rs` |
| **Quick run command** | `cargo test --lib` |
| **Full suite command** | `cargo test` |
| **Estimated runtime** | ~60 s（新增两会话对抗性与断连释放测试） |

**Why no new framework:** 延续 Phase 1 的结论——`cargo test` 足以覆盖单元、集成与端到端重放三类需求；不引入快照自动接受类工具（会让"行为变了"与"夹具跟着变了"难以区分）。

---

## Sampling Rate

- **After every task commit:** `cargo test --lib`
- **After every plan wave:** `cargo test` + `cargo check --target i686-pc-windows-gnu`
- **Before `$gsd-verify-work`:** full suite green；`cargo test --test contract_fixtures` green
- **Max feedback latency:** ~60 s

---

## Per-Task Verification Map

Task IDs are assigned during planning. Each task must satisfy one row.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| TBD | — | 1 | SESS-01 | T-02-01 | 会话 ID 由 `OsRng` 生成、32 hex、两次不同、不下发 | unit | `cargo test session` | ❌ W1 | ⬜ pending |
| TBD | — | 1 | SESS-04 (a) | T-02-02 | `Drop` 注销注册表；无残留条目 | unit | `cargo test --test session_lifecycle` | ❌ W1 | ⬜ pending |
| TBD | — | 2 | SESS-02 | T-02-03 | 两会话对抗：A 验 PIN 后 B 仍被拒 | integration | `cargo test --test session_isolation` | ❌ W2 | ⬜ pending |
| TBD | — | 2 | SESS-03 | T-02-03 | 超过 TTL 后授权失效 | unit | `cargo test session::auth` | ❌ W2 | ⬜ pending |
| TBD | — | 2 | SESS-05 | T-02-04 | 授权条目无 PIN 字段；`ctx.pins` 归零 | structural | `grep -c 'ctx\.pins' src/main.rs` == 0 | ❌ W2 | ⬜ pending |
| TBD | — | 2 | SESS-06 | — | 未授权会话返回稳定错误码 | unit | `cargo test domain::auth` | ❌ W2 | ⬜ pending |
| TBD | — | 3 | RES-01 | T-02-05 | 客户端整数不再被当作句柄接受 | integration | `cargo test domain::device` | ❌ W3 | ⬜ pending |
| TBD | — | 3 | RES-02 | T-02-05 | 未知/过期/类型错误各自被拒且不泄露格式 | unit | `cargo test session::handles` | ❌ W3 | ⬜ pending |
| TBD | — | 3 | RES-03 | T-02-02 | 断连（含异常）后全部句柄释放 | integration | `cargo test --test session_lifecycle` | ❌ W3 | ⬜ pending |
| TBD | — | 3 | RES-04 | T-02-06 | 原生错误路径同样释放（fake 注入失败后断言 `Close*`） | unit | `cargo test domain` | ❌ W3 | ⬜ pending |
| TBD | — | 3 | RES-05 | T-02-03 | A 的 digest 句柄在 B 中不可用 | integration | `cargo test --test session_isolation` | ❌ W3 | ⬜ pending |
| TBD | — | 4 | 契约 | T-02-07 | 37 夹具继续匹配；digest 拒绝消息逐字不变 | integration | `cargo test --test contract_fixtures` | ✅ | ⬜ pending |

**Threat refs** will be defined in each plan's `<threat_model>` block. Provisional mapping:

- **T-02-01** 会话 ID 可预测或泄露给客户端 → 变成可冒用的凭据
- **T-02-02** 会话结束只注销注册表而不释放原生句柄 → 设备句柄泄漏（研究 V-1 的重演）
- **T-02-03** 授权状态通过进程级结构泄漏到其他会话 → SESS-02/05 失效
- **T-02-04** 授权条目保留可恢复的 PIN → 凭据暴露
- **T-02-05** 客户端提供的整数被转换回原生指针 → 进程崩溃（已实测）
- **T-02-06** 迁移时只处理成功路径，错误路径仍漏释放 → RES-04 失效
- **T-02-07** 迁移 19 个分支时夹带行为变更 → 回归不可归因

---

## Wave 0 Requirements

- [ ] `src/session/mod.rs` + `src/session/registry.rs` —— 会话状态（授权表、句柄表）与注册表
- [ ] `src/protocol/mod.rs` + `src/protocol/params.rs` —— `RpcRequest`/`RpcResponse`/`Language` 与类型化参数提取
- [ ] `tests/session_isolation.rs` —— 两会话对抗性测试骨架（SESS-02、RES-05）
- [ ] `tests/session_lifecycle.rs` —— 断连与设备移除的释放测试骨架（SESS-04、RES-03）
- [ ] `FakeSkfProvider` 已具备 `DeviceRemoved`、`PinIncorrect` 注入与调用记录（Phase 1 完成）

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| 真实设备下的会话隔离 | SESS-02 增强 | 无 token 时只能走拒绝路径 | 在有 GM3000 驱动的机器上：会话 A `CheckPIN` 成功 → 会话 B `SignData` 必须被拒（返回未授权错误，而非签名结果） |
| 设备拔出后的失效行为 | SESS-04（b） | 需要真实拔出动作 | 会话 A 授权后拔出 Key → A 的下一次敏感操作必须以 device-not-found 被拒，且授权条目被清除 |
| Windows i686 产物 | D-25 | macOS 无 MSVC | 触发 `release-windows.yml`；`build.ps1` 的 `Machine=0x014C` 断言即证据 |

---

## Validation Sign-Off

- [ ] 每个任务具备 `<automated>` 验证命令或 Wave 0 依赖
- [ ] 采样连续性：不存在连续 3 个任务缺少自动化验证
- [ ] Wave 0 覆盖全部缺失引用
- [ ] 无 watch-mode 标志
- [ ] 反馈延迟 < 60 s
- [ ] 计划完成后置 `nyquist_compliant: true`

**Approval:** pending

---

*Phase: 02-session-authorization-and-resource-ownership*
*Validation strategy created: 2026-09-11*

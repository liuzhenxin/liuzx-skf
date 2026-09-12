---
phase: 3
slug: transport-hardening-and-concurrency
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-09-11
---

# Phase 3 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust 内置 `cargo test`（`#[test]` / `#[tokio::test]`）；复用 library 目标与 `tests/session_support`、`tests/common` 辅助 |
| **Config file** | none — 不引入新框架 |
| **Quick run command** | `cargo test --lib` |
| **Full suite command** | `cargo test` + `cargo check --all-targets` + `cargo check --target i686-pc-windows-gnu` |
| **Estimated runtime** | ~10 s |

---

## Sampling Rate

- **After every task commit:** `cargo test --lib`
- **After every plan wave:** `cargo test` + i686 cross-check
- **Before `$gsd-verify-work`:** full suite green；`cargo test --test contract_fixtures` green
- **Max feedback latency:** ~15 s

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 03-01-01 | 03-01 | 1 | TRANS-01 | T-03-01 | 1 MiB 帧/消息在 tungstenite 分配前被拒 | unit/structural | `grep -c 'accept_async_with_config' src/server/mod.rs` == 1 | ✅ | ⬜ pending |
| 03-01-02 | 03-01 | 1 | TRANS-01 | T-03-02 | 256 KiB 载荷在 base64 decode 前被拒 | unit | `cargo test protocol` | ✅ | ⬜ pending |
| 03-01-03 | 03-01 | 1 | TRANS-02 | T-03-03 | 第 65 个连接被拒绝 | structural | `grep -c 'Semaphore::new' src/server/mod.rs` == 1 | ✅ | ⬜ pending |
| 03-01-04 | 03-01 | 1 | TRANS-01, TRANS-02 | T-03-01/02/03 | 端到端超限拒绝且服务存活 | integration | `cargo test --test transport_limits` | ❌ W1 | ⬜ pending |
| 03-02-01 | 03-02 | 2 | TRANS-05 | T-03-05/08 | 所有 native 调用取锁，wait/cancel 豁免，无新 unsafe | unit/structural | `cargo test --test provider_invariants` | ✅ | ⬜ pending |
| 03-02-02 | 03-02 | 2 | TRANS-04 | T-03-06 | 请求处理体整体在阻塞池执行（含守卫 Drop） | structural/integration | `cargo test --test contract_fixtures` + `cargo test --test session_lifecycle` | ✅ | ⬜ pending |
| 03-02-03 | 03-02 | 2 | TRANS-03 | T-03-06/07 | 非 wait 请求 30s 超时返回 `-1`；wait/cancel 豁免 | unit | `cargo test --test ffi_serialization` | ❌ W2 | ⬜ pending |
| 03-02-04 | 03-02 | 2 | TRANS-03, TRANS-04, TRANS-05 | T-03-05/06/07 | 慢调用不拖累无关请求；wait/cancel 并发 | integration | `cargo test --test ffi_serialization` | ❌ W2 | ⬜ pending |
| 03-03-01 | 03-03 | 3 | TRANS-06 | T-03-12 | `allow_remote` 具名字段 + `SKF_ALLOW_REMOTE` 解析 | unit | `cargo test config` | ✅ | ⬜ pending |
| 03-03-02 | 03-03 | 3 | TRANS-06 | T-03-10/11 | bind 前按解析地址拒绝非回环 | structural | `grep -c 'is_loopback' src/server/mod.rs` == 1 | ✅ | ⬜ pending |
| 03-03-03 | 03-03 | 3 | TRANS-06 | T-03-10/11/13 | 拒绝/允许四种组合 | integration | `cargo test --test loopback_gate` | ❌ W3 | ⬜ pending |
| 03-04-01 | 03-04 | 3 | TRANS-07 | T-03-16 | `OperationClass` + 全量映射 | unit | `cargo test classification` | ❌ W3 | ⬜ pending |
| 03-04-02 | 03-04 | 3 | TRANS-07 | T-03-14 | 与 `EXPECTED_METHODS` 双向一致 | integration | `cargo test --test classification_invariants` | ❌ W3 | ⬜ pending |
| 03-04-03 | 03-04 | 3 | TRANS-07 | T-03-15 | 37 方法文档表 | docs | `grep -c '^| [A-Z]' docs/OPERATION-CLASSIFICATION.md` >= 37 | ❌ W3 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] `tests/session_support/mod.rs` —— 子进程服务 + WS 辅助（Phase 2 已建立）
- [x] `tests/common/mod.rs` —— `EXPECTED_METHODS` 与 normalize（Phase 1 已建立）
- [x] `FakeSkfProvider` 具备 `Blocking(Duration)` 注入与调用记录（Phase 1 已建立）
- [ ] `tests/transport_limits.rs` —— TRANS-01/02 骨架（03-01-04 创建）
- [ ] `tests/ffi_serialization.rs` —— TRANS-03/04/05 骨架（03-02-04 创建）
- [ ] `tests/loopback_gate.rs` —— TRANS-06 骨架（03-03-03 创建）
- [ ] `tests/classification_invariants.rs` —— TRANS-07 骨架（03-04-02 创建）

*Wave 0 只缺本阶段任务自身创建的文件；基础设施已具备，无外部依赖。*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| 真实 GM3000 上的并发调用不崩溃 | TRANS-05 | 无 token；锁有效性只能由 fake/结构测试间接证明 | 在有 GM3000 的机器上并发两个 `SignData`，确认无进程崩溃且结果正确 |
| 缓慢真实设备下无关客户端保持响应 | TRANS-03/04 | 需要真实缓慢设备或人为卡顿 | 用真实 token 触发一个长操作，同时在另一连接执行 `SetLanguage`，确认立即返回 |
| 厂商 DLL 在锁之外是否线程安全 | TRANS-05 | 无法在无 DLL 并发缺陷的情况下证明 | 记录为已知不确定性；若将来获得厂商文档再评估放宽锁 |

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

*Phase: 03-transport-hardening-and-concurrency*
*Validation strategy created: 2026-09-11*

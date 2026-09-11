---
phase: 1
slug: structural-foundation-and-test-seam
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-09-11
---

# Phase 1 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust 内置 `cargo test`（`#[test]` / `#[tokio::test]`；tokio `full` feature 已启用） |
| **Config file** | none — 新增 `src/lib.rs` 后 `cargo test` 即可发现测试；本阶段不引入测试框架 |
| **Quick run command** | `cargo test --lib` |
| **Full suite command** | `cargo test` |
| **Estimated runtime** | ~30 s（无硬件、无网络、无外部进程） |

**Why no new framework:** D-21 约束不引入重量级新依赖；`cargo test` 已能覆盖纯逻辑单测与端到端契约重放两类需求。

---

## Sampling Rate

- **After every task commit:** `cargo test --lib`
- **After every plan wave:** `cargo test` + `cargo check --target i686-pc-windows-gnu`
- **Before `$gsd-verify-work`:** full suite green；`cargo check --target i686-pc-windows-gnu` green
- **Max feedback latency:** ~30 s

---

## Per-Task Verification Map

Task IDs are assigned during planning. This map defines the verification contract each task must satisfy.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 01-01-01 | 01 | 1 | FOUND-03 | T-01-01 | Fixture 由重构前二进制录制，绝不由新 fake 生成 | integration | `cargo test --test fixture_oracle` | ❌ W1 | ⬜ pending |
| 01-01-02 | 01 | 1 | FOUND-03 | T-01-02 | 易变字段在录制时冻结；事后放宽 normalize 视为契约变更 | integration | `cargo test --test contract_fixtures` | ❌ W1 | ⬜ pending |
| 01-02-01 | 02 | 2 | FOUND-02 | — | N/A | build | `test -f src/lib.rs && grep -c "pub mod" src/lib.rs` | ❌ W2 | ⬜ pending |
| 01-02-02 | 02 | 2 | FOUND-06 | — | N/A | unit | `cargo test crypto` | ❌ W2 | ⬜ pending |
| 01-02-03 | 02 | 2 | FOUND-02, D-18/D-19 | — | N/A | unit | `cargo test config` | ❌ W2 | ⬜ pending |
| 01-03-01 | 03 | 3 | FOUND-04 | T-01-03 | trait 不暴露原生指针；库实例唯一 | unit | `grep -c "Library::new" src/provider/native.rs` == 1 | ❌ W3 | ⬜ pending |
| 01-03-02 | 03 | 3 | FOUND-05 | T-01-04 | 五类失败注入各自可断言；fake 记录调用序列 | unit | `cargo test provider::fake` | ❌ W3 | ⬜ pending |
| 01-04-01 | 04 | 4 | FOUND-01, FOUND-02 | — | N/A | build | `cargo test 2>&1 \| grep -E "test result: ok\. [1-9]"` | ❌ W4 | ⬜ pending |
| 01-04-02 | 04 | 4 | D-20 | — | N/A | build | `cargo check --target i686-pc-windows-gnu` | ❌ W4 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

**Threat refs** are drawn from the per-plan `<threat_model>` blocks: T-01-01 (tautological fixtures defeat the regression oracle), T-01-02 (post-hoc normalization silently deletes assertions), T-01-03 (native pointer crossing the provider boundary), T-01-04 (over-compliant fake leaving failure paths untested).

---

## Wave 0 Requirements

本阶段本身就是"建立验证基础设施"，因此 Wave 0 与阶段主干重合：

- [ ] `src/lib.rs` + 模块骨架 — 缺少它 `cargo test` 无法编译任何 Rust 测试（FOUND-01/02 的根因）
- [ ] `tools/record_fixtures`（脚本或临时测试二进制）— 在改动生产代码**之前**用当前代码录制响应
- [ ] `tests/fixtures/v0.2.0/*.json` — 录制产物，含冻结的 `normalize` 与 `provenance`
- [ ] `tests/contract_fixtures.rs` — 端到端重放驱动（fake provider + 真实服务器 + `127.0.0.1:0`）
- [ ] `tests/fixture_oracle.rs` — 证明 oracle **能够失败**的负向自检
- [ ] `run_server` provider 注入点 + `bind()` 暴露 `local_addr()` — 临时端口测试的前置条件

**Critical ordering:** 录制与 fixture 提交必须发生在任何生产代码重构之前。这是 D-09/D-10 的执行含义，也是本阶段唯一的不可逆顺序约束。

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| 真实 GM3000 设备下的成功路径契约 | FOUND-03（增强） | 托管/开发机无驱动与设备；无设备时只能录到确定性的错误契约（D-11） | 在装有 GM3000 驱动的 Windows 机器上重跑 `tools/record_fixtures`，比对既有 fixture，把 `observed_with_device` 置为 true 并提交差异 |
| `i686-pc-windows-msvc` 正式打包与产物名不变 | D-20 | macOS 无 MSVC 工具链 | 在 Windows 构建机执行 `powershell -ExecutionPolicy Bypass -File .\packaging\windows\build.ps1`，确认输出 `dist\skf-service-windows-x64-gm3000-x86\skf-service.exe` 且 PE Machine 为 `0x014C` |
| 真实服务手工冒烟（WS + HTTP Demo） | D-18 | 端到端 fixture 重放使用 fake provider，不覆盖真实库加载 | 运行 `cargo run`，确认 `ws://127.0.0.1:9001` 与 `http://0.0.0.0:8000/skf_api.html` 可达，`EnumDevice("GM3000")` 返回与重构前一致的结果 |

---

## Validation Sign-Off

- [ ] 每个任务具备 `<automated>` 验证命令或 Wave 0 依赖
- [ ] 采样连续性：不存在连续 3 个任务缺少自动化验证
- [ ] Wave 0 覆盖全部缺失引用
- [ ] 无 watch-mode 标志
- [ ] 反馈延迟 < 30 s
- [ ] 计划完成后置 `nyquist_compliant: true`

**Approval:** pending

---

*Phase: 01-structural-foundation-and-test-seam*
*Validation strategy created: 2026-09-11*

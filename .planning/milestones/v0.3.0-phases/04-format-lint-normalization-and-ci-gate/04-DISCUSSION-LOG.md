# Phase 4: Format/Lint Normalization and CI Gate - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-09-11
**Phase:** 04-format-lint-normalization-and-ci-gate
**Areas discussed:** 格式化, Clippy 基线, CI 工作流, 门禁强制证明
**Mode:** 用户选择 "全部采用推荐"

---

## 1. 格式化范围与配置（QUAL-01）

| Option | Description | Selected |
|--------|-------------|----------|
| **rustfmt 默认 + 一次性全仓储 + isolated commit** | 233 处 diff 全部来自默认风格；先于 lint 修复 | ✓ |
| 自定义 `rustfmt.toml` | 扩大一次性 diff，制造风格争论 | |
| 只格式化近期改动文件 | 不满足 "repository-wide" | |

**User's choice:** 1a
**Notes:** 格式化必须是独立提交并声明"仅格式"；随后用 `cargo test` 证明无语义变更。

---

## 2. Clippy 基线与工具链（QUAL-02）

| Option | Description | Selected |
|--------|-------------|----------|
| **修复全部 + scoped allow + 固定工具链** | CI `-D warnings`；仅 C ABI 签名做 allow | ✓ |
| 用 crate 级 allow 抑制 | 文档成本低但掩盖问题 | |

**User's choice:** 2a
**Notes:** 侦察得到 63 条 warning：`get_first`×25、`needless_return`×12、`needless_borrows_for_generic_args`×9、`await_holding_lock`×4、`too_many_arguments`×2、其余各 1。`too_many_arguments` 只出现在镜像 C ABI 的 `encrypt_data`/`decrypt_data`，用 scoped allow + 注释。新增 `rust-toolchain.toml` 固定 1.93.0。

---

## 3. CI 工作流设计（QUAL-03）

| Option | Description | Selected |
|--------|-------------|----------|
| **契约重放在 x86_64 macOS；Linux 只跑硬件无关子集** | 厂商 dylib 已提交且为 x86_64；Linux 缺 `.so` 会让 37 夹具漂移 | ✓ |
| Linux 跑全部 | 契约重放必然失败 | |
| 缺厂商库时跳过契约重放 | 削弱冻结契约的门禁价值 | |

**User's choice:** 3a
**Notes:** 触发 `pull_request` + push 到 `main` 与 `dev`。Jobs：`fmt`、`clippy`、`test`（ubuntu，硬件无关）、`test-full`（x86_64 macOS，含契约重放）、`windows-i686`（ubuntu + mingw）、`gate-selftest`。runner 标签实现时确认。

---

## 4. 门禁强制证明（QUAL-04）

| Option | Description | Selected |
|--------|-------------|----------|
| **可执行 self-test job** | 写入必失败测试，断言门禁退出码非零，再清理；job 本身绿 | ✓ |
| 仅文档声明 | 不可验证 | |

**User's choice:** 4a
**Notes:** 分支保护无法由仓库文件设置，写入 `docs/CI.md` 作为必需设置清单；self-test 证明"检查会失败"，分支保护负责"阻断合并"。

---

## the agent's Discretion

- CI YAML 结构与 step 命名；是否使用 Cargo 缓存（键含工具链哈希）。
- 删除块尾 `return` 时的分支合并形状。
- `docs/CI.md` 的排版。

## Deferred Ideas

- 前端 `pki-ui` 的 ESLint/Prettier 门禁（另一仓库）。
- `tools/record_fixtures.js` 的 lint/格式化。
- `cargo deny` / 依赖审计（阶段 6）。
- 覆盖率阈值。
- `macos-13` 退役后的 runner 迁移。
- CI 中的 Windows 实机测试（阶段 5）。

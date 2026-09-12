# Phase 4: Format/Lint Normalization and CI Gate - Context

**Gathered:** 2026-09-11
**Status:** Ready for planning

<domain>
## Phase Boundary

建立真实生效的合并门禁：格式与 lint 基线干净，CI 在 PR 与主分支推送时运行硬件无关测试，并在失败时阻断合并。

具体交付（QUAL-01..04）：

1. **QUAL-01**：全仓储 `cargo fmt --all -- --check` 通过，且格式化是隔离的 formatting-only 变更。
2. **QUAL-02**：`cargo clippy --all-targets` 无 actionable warning；任何保留的 `allow` 都显式限定作用域并注明原因。
3. **QUAL-03**：CI 工作流在 `pull_request` 与 `main`/`dev` 推送时运行 fmt、clippy、硬件无关测试与 i686 Windows 编译检查。
4. **QUAL-04**：一个有意的失败测试确实会让门禁变红（可验证），而不是"仅文档声明"。

**本阶段不做**：修改任何运行时代码行为；Windows 服务就绪（阶段 5）；日志/诊断/发布完整性（阶段 6）；前端 `pki-ui` 的 ESLint/Prettier（不同仓库）。

**硬约束**：格式化与 lint 修复**不得夹带行为变更**；37 个 v0.2.0 夹具必须继续匹配。

</domain>

<decisions>
## Implementation Decisions

### 格式化（QUAL-01，区域 1）
- **D-01:** 使用 **rustfmt 默认配置**，不加 `rustfmt.toml`。当前有 233 处 diff，全部来自默认风格；引入自定义宽度只会扩大一次性 diff 并制造后续争论。
- **D-02:** 全仓储格式化作为**一个独立的 formatting-only 提交**，且**先于**任何 lint 修复落地。提交信息必须声明"仅格式，无逻辑变更"，便于 `git blame` 与 review 归因。
- **D-03:** 格式化后立即用 `cargo test`（37/37 夹具）证明无语义变更。

### Clippy 基线与工具链（QUAL-02，区域 2）
- **D-04:** **修复全部 63 条 warning**，不整类抑制。分类与处置：
  - `get_first`×25 → 用 `.first()`；
  - `needless_return`×12 → 删除块尾 `return`；
  - `needless_borrows_for_generic_args`×9 → 去掉多余 `&`；
  - `await_holding_lock`×4（`tests/loopback_gate.rs`）→ 把测试内的 env 串行锁从 `std::sync::Mutex` 改为 `tokio::sync::Mutex`（异步测试中持 `std` 锁跨 await 是真实隐患）；
  - `single_match`×1（`tests/transport_limits.rs`）→ 改 `if let`；
  - `bool_assert_comparison`×1（`src/protocol/params.rs`）→ 改 `assert!`；
  - `for_kv_map`×1 → 用 `.values()`；
  - `collapsible_str_replace`×1（`src/domain/container.rs`）→ `replace(['\n','\r'], "")`；
  - `missing_safety_doc`×1（`src/skf/api.rs::get_func`）→ 补 `# Safety` 段；
  - `too_many_arguments`×2（`src/skf/api.rs::encrypt_data`/`decrypt_data`）→ **scoped `#[allow(clippy::too_many_arguments)]` + 注释**，因为参数列表镜像厂商 C ABI 签名，重构会破坏 ABI 对应关系。
- **D-05:** CI 中 `cargo clippy --all-targets -- -D warnings`，使 warning 成为硬失败。
- **D-06:** 新增 **`rust-toolchain.toml` 固定 `channel = "1.93.0"`**，`components = ["rustfmt", "clippy"]`。理由：clippy lint 集合随工具链演进而变化；不固定则 CI 镜像升级会在无代码变更时把门禁变红。

### CI 工作流（QUAL-03，区域 3）
- **D-07:** 新增 `.github/workflows/ci.yml`，触发：
  - `pull_request`（所有目标分支）；
  - `push` 到 `main` **与 `dev`**（当前集成分支是 `dev`；ROADMAP 只写了 main，补充 dev 是刻意决定）；
  - `permissions: contents: read`，并用 `concurrency` 取消被取代的运行。
- **D-08:** Job 设计（runner OS 是关键）：
  | Job | Runner | 命令 |
  |-----|--------|------|
  | `fmt` | ubuntu-latest | `cargo fmt --all -- --check` |
  | `clippy` | ubuntu-latest | `cargo clippy --all-targets -- -D warnings` |
  | `test` | ubuntu-latest | `cargo test --lib` + `--test provider_invariants --test session_invariants --test classification_invariants`（无厂商库依赖） |
  | `test-full` | **x86_64 macOS runner** | `cargo test`（含 `contract_fixtures`） |
  | `windows-i686` | ubuntu-latest | 安装 `mingw-w64` 后 `cargo check --target i686-pc-windows-gnu` |
  | `gate-selftest` | ubuntu-latest | 见 D-09 |
- **D-09:** **契约重放必须在 x86_64 macOS runner 上运行**。`native/GM3000/macos/x86_64/libgm3000.1.0.dylib` 已提交且为 x86_64；`contract_fixtures` 启动真实二进制并期望加载它。Linux runner 缺 Linux `.so`，会加载失败并让 37 个夹具全部漂移。**runner 标签在实现时确认**（历史上 `macos-13`；若不可用则用 `macos-15-intel` 等 x86_64 标签），计划必须包含"确认该标签可用"的一步。
- **D-10:** `test` job 与 `test-full` 的分工：前者是 Linux 上的快速硬件无关信号，后者是唯一的权威全量（含冻结契约）。两者都必须绿。

### 门禁强制证明（QUAL-04，区域 4）
- **D-11:** 新增 `gate-selftest` job：在工作区写入一个临时 `tests/ci_selftest.rs`，内容为必然失败的 `#[test] fn deliberate_failure() { assert!(false); }`，运行 `cargo test --test ci_selftest`，**断言其退出码非零**；随后删除该文件。job 本身绿（因为它验证的是"门禁会红"），从而把 QUAL-04 从文档声明变成可执行证据。
- **D-12:** 分支保护**无法由仓库文件设置**。新增 `docs/CI.md`，列出 GitHub 仓库必需设置（对 `main` 要求本工作流的 job 通过、禁止直接推送），并说明 `gate-selftest` 的证明范围（它能证明检查会失败；真正阻断合并依赖该设置）。

### 文档与本地等价
- **D-13:** 新增 `docs/CI.md`：门禁总览、各 job 用途、本地等价命令（`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`）、分支保护清单、以及"契约重放为何需要 x86_64 macOS"。

### the agent's Discretion
- CI YAML 的具体结构与 step 命名。
- 是否用 `actions/cache`/`Swatinem/rust-cache` 缓存 Cargo；若用，键应包含 `rust-toolchain.toml` 的哈希。
- 删除块尾 `return` 时是否合并相邻分支的形状。
- `docs/CI.md` 的排版。

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### 阶段范围与验收标准
- `.planning/ROADMAP.md` §Phase 4 — 目标、依赖、4 条成功标准
- `.planning/REQUIREMENTS.md` §Code Quality and CI — QUAL-01..04
- `.planning/PROJECT.md` — Constraints（i686 兼容、发布产物、CI 边界）

### 前一阶段产出（本阶段直接依赖）
- `.planning/phases/03-transport-hardening-and-concurrency/03-VERIFICATION.md` — 当前测试矩阵（133 测试、37 夹具）
- `.planning/phases/03-transport-hardening-and-concurrency/03-REVIEW.md` — 已知接受的 lint/结构点
- `.planning/phases/01-structural-foundation-and-test-seam/01-VERIFICATION.md` — 契约重放需要厂商库的事实来源

### 现有 CI 与工具链
- `.github/workflows/release-windows.yml` — 现有 workflow 风格、`actions/checkout@v4`、`upload-artifact@v4`
- `Cargo.toml` — edition 2021、dev-dependency `test-provider`、目标平台
- `tests/contract_fixtures.rs` — 启动真实二进制、从 stdout 读端口；契约重放对厂商库的依赖
- `tests/fixtures/v0.2.0/README.md` — 契约纪律与 `normalize` 冻结

### 现有源码（lint 修复落点）
- `src/skf/api.rs` — `get_func` 缺 `# Safety`；`encrypt_data`/`decrypt_data` 参数镜像 C ABI
- `src/domain/container.rs` — `collapsible_str_replace`
- `src/protocol/params.rs` — `bool_assert_comparison`
- `tests/loopback_gate.rs` — `await_holding_lock`（`std` 锁跨 await）
- `tests/transport_limits.rs` — `single_match`
- `src/main.rs` — 大量 `get_first` / `needless_return` / `needless_borrows_for_generic_args`
- `packaging/windows/build.ps1` — i686 产物的既有断言（阶段 5/6 复用）

**本项目无外部规格或 ADR 文档** —— 需求与约束完整记录于上述 `.planning/` 文件与源码本身。

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **既有 workflow 模板**（`.github/workflows/release-windows.yml`）：`actions/checkout@v4`、`shell` 指定、`permissions` 块，可直接沿用风格。
- **硬件无关测试集合**：`cargo test --lib`（87）、`provider_invariants`（7）、`session_invariants`（6）、`classification_invariants`（4）都不需要厂商库。
- **契约重放**：`tests/contract_fixtures.rs` 与 `tests/fixture_oracle.rs` 是唯一的权威门禁；前者需要厂商库存在。
- **`packaging/windows/build.ps1` 的 `Machine=0x014C` 断言**：i686 目标的语义来源；CI 的 `cargo check --target i686-pc-windows-gnu` 是它的快速替代。

### Established Patterns
- 契约纪律：任何行为漂移必须显式声明；`normalize` 冻结。
- 提交风格：Conventional Commits（`feat(...)`、`fix(...)`、`test(...)`、`docs(...)`、`refactor(...)`），本阶段会有一次纯 `style:` 格式化提交。
- 反模式（阶段 2/3 记录）：脚本化多点改写必须逐 hunk 阅读 diff；裸 grep 不能作为唯一证据。

### Integration Points
- `.github/workflows/ci.yml` 是唯一新增 workflow；`release-windows.yml` 不动。
- `rust-toolchain.toml` 在仓库根，被本地与 CI 同时读取。
- `docs/CI.md` 与 `packaging/windows/` 的文档并列。
- clippy 修复触及 `src/main.rs`、`src/skf/api.rs`、`src/domain/container.rs`、`src/protocol/params.rs` 与两个测试文件。

</code_context>

<specifics>
## Specific Ideas

- 用户明确要求格式化**先于** lint 修复、且隔离提交，以便归因。
- 用户接受**修复全部** clippy warning，只对镜像 C ABI 的 `too_many_arguments` 做 scoped allow。
- 用户要求**固定工具链**，避免 clippy 基线随 CI 镜像漂移。
- 用户接受**契约重放放在 x86_64 macOS runner**，并认识到 Linux runner 缺厂商 `.so`。
- 用户要求用**可执行的 self-test job** 证明门禁会红，而非仅写文档。
- 用户接受分支保护只能文档化（仓库文件无法设置）。

</specifics>

<deferred>
## Deferred Ideas

- **前端 `pki-ui` 的 ESLint/Prettier 门禁** → 属于父级 PKI 工作区的另一仓库，不在本阶段。
- **Node 录制脚本（`tools/record_fixtures.js`）的 lint/格式化** → 非必要，未纳入 QUAL-01/02。
- **`cargo deny` / 依赖审计** → 阶段 6 发布完整性可考虑。
- **覆盖率报告/门槛** → 本阶段只建立门禁，不设覆盖率阈值。
- **`macos-13` 退役后的 runner 迁移** → 实现时确认标签；若需长期方案，在阶段 6 评估固定 self-hosted 或预编译契约数据。
- **CI 中运行 Windows 实机测试** → 阶段 5（Windows Service Reliability）。

</deferred>

---

*Phase: 04-format-lint-normalization-and-ci-gate*
*Context gathered: 2026-09-11*

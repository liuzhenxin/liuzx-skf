# Phase 1: Structural Foundation and Test Seam - Context

**Gathered:** 2026-09-11
**Status:** Ready for planning

<domain>
## Phase Boundary

让代码库首次可被自动化验证：把 crate 拆分为 library + thin binary，使模块可脱离二进制入口单测；为 v0.2.0 已发布行为建立回归防线；把 SKF 调用隔离在可替换的 provider 抽象之后，并提供可注入失败的 fake 实现。

**本阶段不做**：会话与授权改造、原生句柄归属重设计、传输限制、并发/阻塞隔离、格式化与 lint 归一化、Windows 服务就绪状态、日志轮转、发布校验、协议版本协商。这些分属阶段 2–6。

**本阶段行为约束**：`handle_request` 及其 37 个方法分支的**运行时行为不得改变**。所有改动可被已录制的 v0.2.0 fixture 证明为等价。

</domain>

<decisions>
## Implementation Decisions

### Provider 抽象形态
- **D-01:** 采用**全量领域操作 trait**。trait 方法按业务动作定义（枚举设备、打开应用、验证 PIN、签名、摘要、加解密等），而非逐符号镜像 SKF C ABI。`NativeSkfProvider` 内部包装现有 `SkfApi`，`SkfApi` 退化为 provider 的内部实现细节。
- **D-02:** trait 的句柄类型使用 **provider 自定义 newtype**（例如 `DeviceHandle(u64)`），原生指针完全封在 provider 内部，不越过 trait 边界。这是为阶段 2 的 RES-01（不透明标识）铺路。
- **D-03:** provider 层定义 **typed error enum**，内部保留 SKF 十六进制返回码作为细节字段。阶段 6 的稳定错误码将建立在这层之上。
- **D-04（明确排除）:** 不采用逐符号镜像 trait。它迁移成本低但测试需要构造裸指针，测试价值低，且阶段 2 的句柄归属仍需重做。

### Phase 1 重构范围
- **D-05:** 本阶段只移动三类内容：`src/main.rs` 约 353–540 行的**纯编码函数**（`hex_to_bytes`、`der_*`、`build_subject_dn`、`build_sm2_spki`、`build_rsa_spki`）迁入 `crypto/`；配置加载与 `%VAR%` 展开迁入 `config/`；新增 `provider/`。`run_server` 接受**可注入的 provider**。
- **D-06:** `handle_request` 与 37 个方法分支**保留在 `main.rs`**，本阶段不迁移。
- **D-07:** 协议层抽取（`RpcRequest`/`RpcResponse`/`Language`/参数解析 → `protocol/`）**推迟到阶段 2**。
- **D-08:** 领域处理函数抽取（37 分支 → `domain/`）**推迟到阶段 2**，与阶段 2 的会话改造合并为一次改动。

### v0.2.0 契约冻结
- **D-09:** 采用**重构前录制 + 端到端重放**。在改动任何生产代码之前，用当前 `v0.2.0` 代码启动服务、逐方法发送真实请求、把完整响应原样入库为基线。阶段 1 内部由此形成**硬性顺序：先录制、先提交 fixture，再开始重构**。
- **D-10（反循环论证约束）:** fixture **绝不允许**由阶段 1 新写的 fake provider 生成。若用新 fake 生成 fixture 再断言同一 fake 的输出，测试不证明任何东西。这一条必须在计划与代码评审中显式校验。
- **D-11:** 需要硬件的方法，在无设备时返回的**确定性错误响应**（如设备未发现）同样构成有效契约，按实际响应录制，不跳过。
- **D-12:** fixture 存放于 `tests/fixtures/v0.2.0/<Method>.json`，每个文件包含 `request`、`response`、`provenance` 字段；`provenance` 记录录制来源与是否依赖硬件。
- **D-13:** fixture 以**端到端**方式重放：用可注入 provider 启动真实服务器到临时端口（`127.0.0.1:0`），发送真实请求，比对完整响应 JSON。

### Fake provider 覆盖范围
- **D-14:** fake 只实现测试实际消费的**方法子集**，但**失败注入 API 从第一天完备**。FOUND-05 要求的是「可注入失败」，不是「全方法可 fake」。
- **D-15:** 失败注入按调用配置，形如 `fail_next(Operation, SkfError)`，可与正常行为组合使用。
- **D-16:** fake **记录调用序列**，供测试断言资源释放与调用顺序。阶段 2 验证 RES-03/RES-04 依赖此能力。
- **D-17:** 本阶段不追求 37 方法全覆盖的 fake。覆盖面随阶段 2/3 的测试需求增长。

### 兼容性约束（延续既有决策，非本阶段新决策）
- **D-18:** 环境变量 `SKF_CONFIG`、`SKF_WS_ADDR`、`SKF_HTTP_ADDR`、`RUST_LOG` 必须继续生效。
- **D-19:** `config/skf.yaml` 与 `packaging/windows/skf-windows-x86.yaml` 必须继续可解析，键名与语义不变。
- **D-20:** FFI 符号名、C 结构布局与调用约定保持规范兼容；`i686-pc-windows-gnu` 编译检查与 `i686-pc-windows-msvc` 打包必须继续通过。
- **D-21:** 不引入重量级新依赖；阶段 1 不新增运行时必需的第三方 crate。

### the agent's Discretion
- 新模块的确切文件名与目录嵌套层级
- trait 方法的具体命名与签名细节
- fixture 测试的辅助函数与测试组织方式
- `provenance` 字段的具体编码格式
- fake provider 的内部数据结构与调用记录格式
- 纯函数提取时是否顺带补充文档注释

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### 阶段范围与验收标准
- `.planning/ROADMAP.md` §Phase 1 — 阶段目标、依赖、成功标准（6 条可观测行为）
- `.planning/REQUIREMENTS.md` §Foundation — FOUND-01..06 是本阶段全部需求
- `.planning/PROJECT.md` — 里程碑目标、Constraints（Compatibility/Hardware/Security/FFI/API/Operations/Privileges）、Key Decisions

### 研究与陷阱（决定本阶段做法）
- `.planning/research/STACK.md` §1（`src/lib.rs` + thin binary）、§5（测试基础设施与 fake 设计）、§What NOT to Add — 依赖取舍与 fake provider 设计要求
- `.planning/research/ARCHITECTURE.md` §Target Architecture、§Component Boundaries、§Build Order Implications — 目标分层与构建顺序
- `.planning/research/PITFALLS.md` — Pitfall 1（无测试重构）、Pitfall 3（重构与语义变更混合）、Pitfall 4（破坏 v0.2.0 契约）、Pitfall 5（句柄跨任务共享）、Pitfall 12（过度顺从的 fake）
- `.planning/research/SUMMARY.md` §Phase 1 — 阶段理由与规避目标

### 代码库现状（决定移动什么、保持什么）
- `.planning/codebase/ARCHITECTURE.md` — 当前 `SkfContext`/`SkfApi`/`handle_request` 边界与数据流
- `.planning/codebase/STRUCTURE.md` — 目录布局与"新代码应放在哪里"
- `.planning/codebase/CONVENTIONS.md` — 命名、错误处理、日志与同步要求（新增 API 需同步更新客户端与测试）
- `.planning/codebase/TESTING.md` — 现有 Node 测试结构、覆盖缺口、已知弱断言
- `.planning/codebase/CONCERNS.md` — 单体分发器、`unsafe impl Send/Sync`、`hash_handles` 全局状态等既有风险

### 平台与打包硬约束
- `packaging/windows/README-Windows-x86_64.md` — PE32/i386（`Machine=0x014C`）组合约束与排障
- `packaging/windows/build.ps1` — 构建时架构校验逻辑
- `config/skf.yaml` 与 `packaging/windows/skf-windows-x86.yaml` — 必须保持可解析的两份配置
- `.cargo/config.toml` — MinGW 交叉编译链接器配置
- `.github/workflows/release-windows.yml` — 现有发布流程，阶段 1 不得破坏

### 现有源码（提取与抽象的输入）
- `src/main.rs` — 353–540 行为纯编码函数（提取目标）；541 行起为 `handle_request`（本阶段保留原地）；28–155 行为 `SkfContext` 与配置展开
- `src/skf/api.rs` — 50 个逐符号薄封装方法，将成为 `NativeSkfProvider` 的内部实现
- `src/skf/types.rs` — `#[repr(C)]` 类型与 SKF 返回码常量
- `Cargo.toml` — 当前无 `[lib]` 段，这是零 Rust 测试的直接原因
- `tests/test_all_apis.js`、`api/skf_api.js` — 请求/响应参数顺序与形状的现有参考

**本项目无外部规格或 ADR 文档** —— 需求与约束完整记录于上述 `.planning/` 文件与源码本身。

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `src/main.rs` 353–540 行的纯函数（`hex_to_bytes`、`der_encode_integer`、`der_encode_length`、`der_wrap`、`der_sequence`、`der_set`、`der_oid`、`der_utf8_string`、`der_printable_string`、`der_bit_string`、`der_small_integer`、`der_context_0`、`build_subject_dn`、`build_sm2_spki`、`build_rsa_spki`）：已是纯函数、无 I/O、无 FFI，是 `crypto/` 的首批提取目标，可立即获得高价值单测。
- `src/skf/api.rs` 的 `SkfApi`：已是逐符号薄封装（每个方法仅做 `get_func` + 调用 + 返回码透传），适合直接作为 `NativeSkfProvider` 的内部实现，无需重写。
- `src/skf/types.rs` 的 `#[repr(C)]` 类型与 `SAR_*`/`SGD_*` 常量：provider 错误枚举与算法标识的直接来源。
- `src/main.rs` 的 `expand_env_vars`：纯字符串处理，可无痛迁入 `config/` 并立即获得单测（含未闭合 `%`、未知变量等边界）。
- `tests/test_all_apis.js` 与 `api/skf_api.js`：记录了每个方法的真实参数顺序与响应形状，是编写请求 fixture 的现成参照。

### Established Patterns
- 错误处理：启动/运维路径用 `anyhow` + context；请求边界用 `RpcResponse::err(code, msg, id)` 早返回。
- Native 失败：SKF 返回码与 `SAR_OK` 比较，十六进制保留在消息中；缺失符号归一为 `SAR_COULDNOTGETFUNCADDR`。
- 平台门控：`#[cfg(windows)]` 用于 `win_service` 模块与 Windows 专属依赖，使 Linux CI 仍可编译测试。
- 配置：`serde_yaml` + `serde(flatten)` 的 provider map，`get_lib_path` 按 `std::env::consts::OS` 选择路径。
- 单元测试目前完全缺失，`cargo test` 运行 0 个测试。

### Integration Points
- `run_server(shutdown)` 是前台与 Windows 服务的**共同**启动路径 —— provider 注入点必须落在这里，两条路径才能同时获得可测试性。
- `SkfContext::new(config)` 是当前唯一的上下文构造点，将成为接受 provider 的位置。
- 纯函数当前被 `handle_request` 的多处分支调用；迁移后需保持相同的调用签名语义，避免夹带行为变更。
- 帧/连接/超时限制与日志轮转的插入点分别位于接受循环与服务模块，本阶段不触碰。
- `Cargo.toml` 需新增 `[lib]` 目标；库名将影响 `main.rs` 的 `use` 路径与 Windows 打包产物名（产物名 `skf-service.exe` 不得改变）。

</code_context>

<specifics>
## Specific Ideas

- 用户强调**契约测试必须防循环论证**：fixture 的真值只能来自重构前的代码，不能来自新建的 fake。这被视为阶段 1 的首要纪律，而非可选建议。
- 阶段 1 内部存在硬性顺序：**录制并提交 fixture → 再动生产代码**。第一个可交付物就是回归防线，而不是重构结果。
- 明确接受「阶段 1 结束时 `handle_request` 仍在 `main.rs`」这一状态。用户认可这是刻意的范围控制，而非未完成的工作——理由是避免对同一批代码做两次高风险改动。
- 明确拒绝逐符号镜像 trait，理由是"看似省事，实际是把两次重构做成一次"。
- 无硬件方法用其真实的确定性错误响应作为契约内容，不跳过。

</specifics>

<deferred>
## Deferred Ideas

- **协议层抽取**（`protocol/`：`RpcRequest`/`RpcResponse`/`Language`/参数解析）→ 阶段 2
- **领域层抽取**（37 分支 → `domain/`）→ 阶段 2，与会话改造合并
- **会话注册表与授权隔离** → 阶段 2（SESS-01..06）
- **不透明资源标识与确定性释放** → 阶段 2（RES-01..05）
- **传输限制、超时、阻塞 FFI 移出异步 worker、按设备串行化** → 阶段 3（TRANS-01..07）
- **格式化与 Clippy 归一化** → 阶段 4（QUAL-01/02）
- **`tempfile` / `proptest` 等测试依赖引入** → 视阶段 1 实际需要决定，不在本阶段强制
- **稳定错误码对外暴露与协议版本协商** → 阶段 6（REL-01/02）；阶段 1 只建立 typed error 基础
- **`IssueCertificate` 双证书正确性修复** → 不在任何阶段；保持 Mock 定位
- **`serde_yaml` 废弃依赖替换** → 独立里程碑，不并入加固版本

</deferred>

---

*Phase: 01-structural-foundation-and-test-seam*
*Context gathered: 2026-09-11*

# Phase 1: Structural Foundation and Test Seam - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-09-11
**Phase:** 01-structural-foundation-and-test-seam
**Areas discussed:** Provider 抽象形态, Phase 1 重构范围, v0.2.0 契约冻结方式, Fake provider 覆盖范围

---

## Provider 抽象形态

| Option | Description | Selected |
|--------|-------------|----------|
| 全量领域 trait | trait 按业务动作定义（枚举设备、打开应用、验证 PIN、签名、摘要、加解密）；`NativeSkfProvider` 内部包装现有 `SkfApi` | ✓ |
| 部分领域 trait | 只抽象有测试需求的方法，其余继续直连 `SkfApi` | |
| 逐符号镜像 | trait 直接镜像 50 个 SKF 函数签名（裸指针、ULONG 返回码） | |

**User's choice:** 全量领域 trait
**Notes:** 用户接受「需要一次性梳理 37 个方法用到的 SKF 调用面」这一前置成本，换取阶段 2 句柄归属的正确基座。明确否决逐符号镜像，理由记录为「看似省事，实际是把两次重构做成一次」；同时否决部分 trait，因为 FOUND-04 要求 SKF 操作经 provider 抽象访问，部分抽象会迫使阶段 2 回补。

### 子决策 1-i：trait 的句柄类型

| Option | Description | Selected |
|--------|-------------|----------|
| provider 自定义 newtype | 如 `DeviceHandle(u64)`，原生指针封在 provider 内部 | ✓ |
| 沿用现有 `SendHandle` | 继续包装原生指针，阶段 2 再换 | |

**User's choice:** provider 自定义 newtype
**Notes:** 为阶段 2 的 RES-01（不透明标识）预留正确形态，避免阶段 2 再次改动 trait 签名。

### 子决策 1-ii：错误类型

| Option | Description | Selected |
|--------|-------------|----------|
| provider 层 typed error enum | 内部保留 SKF 十六进制返回码 | ✓ |
| `ULONG` 返回码 + `anyhow` | 维持现状 | |

**User's choice:** provider 层 typed error enum
**Notes:** 阶段 6 的稳定错误码（REL-02）依赖此基础；本阶段只建立类型，不对外暴露。

---

## Phase 1 重构范围

| Option | Description | Selected |
|--------|-------------|----------|
| 纯逻辑 + config + provider 注入点 | 353–540 行纯函数 → `crypto/`；配置 → `config/`；新增 `provider/`；`run_server` 接受可注入 provider；37 分支留在 `main.rs` | ✓ |
| 上述 + 协议层 | 额外抽出 `RpcRequest`/`RpcResponse`/`Language`/参数解析 → `protocol/` | |
| 上述 + 协议层 + 领域层 | 37 个方法分支全部迁入 `domain/` | |

**User's choice:** 纯逻辑 + config + provider 注入点
**Notes:** 用户接受「阶段 1 结束时 `handle_request` 仍在 `main.rs`」，并明确将其视为刻意范围控制而非未完成工作。核心理由：37 个分支的搬迁与阶段 2 的会话改造触及同一批代码，现在搬一次、阶段 2 再改一次等于同一段逻辑两次高风险改动。契约 fixture 采用端到端形态，只要求 provider 可注入，不要求先抽出协议层或领域层。

---

## v0.2.0 契约冻结方式

| Option | Description | Selected |
|--------|-------------|----------|
| 重构前录制 + 端到端重放 | 动生产代码之前用当前 v0.2.0 代码录制完整响应为基线，之后每次重构重放同一批 fixture | ✓ |
| 重构过程中随时录制 | 无法区分行为变化与 fixture 跟随变化 | |
| 手工编写 schema 断言 | 不依赖运行，但只验证结构不验证取值 | |

**User's choice:** 重构前录制 + 端到端重放
**Notes:** 讨论中明确指出循环论证风险——若用阶段 1 新写的 fake 生成 fixture、再断言同一 fake 的输出，测试不证明任何东西。fixture 真值只能来自重构前的代码，该约束被升级为阶段 1 的首要纪律。由此在阶段 1 内部形成硬性顺序：先录制并提交 fixture，再动生产代码。

### 子决策 3-i：无硬件方法的真值来源

| Option | Description | Selected |
|--------|-------------|----------|
| 录制真实错误响应 | 无设备时返回的确定性错误（如设备未发现）同样构成有效契约 | ✓ |
| 跳过无硬件方法 | 只冻结有硬件的方法 | |

**User's choice:** 录制真实错误响应
**Notes:** 不跳过任何方法，保证 37 个方法分支全部处于回归防线之内。

### 子决策 3-ii：fixture 存放与格式

| Option | Description | Selected |
|--------|-------------|----------|
| 每方法一文件 | `tests/fixtures/v0.2.0/<Method>.json`，含 `request`/`response`/`provenance` | ✓ |
| 单一聚合文件 | 所有方法合并在一个 JSON 中 | |

**User's choice:** 每方法一文件
**Notes:** 按方法分文件便于逐个方法的差异审查与失败定位；`provenance` 记录录制来源与是否依赖硬件。

---

## Fake provider 覆盖范围

| Option | Description | Selected |
|--------|-------------|----------|
| 最小必要方法 + 完整失败注入能力 | fake 只实现测试消费的方法子集，但失败注入 API 从第一天完备 | ✓ |
| 全 37 方法可 fake | 阶段 1 体量显著变大，多数方法本阶段无测试消费者 | |
| 暂不建 fake | 只做纯逻辑测试 | |

**User's choice:** 最小必要方法 + 完整失败注入能力
**Notes:** FOUND-05 要求的是「可注入失败」而非「全方法可 fake」；覆盖面随阶段 2/3 的测试需求增长，能力先到位。

### 子决策 4-i：失败注入形态

| Option | Description | Selected |
|--------|-------------|----------|
| 按调用配置 | `fail_next(Operation, SkfError)`，可组合 | ✓ |
| 构造时固定一组失败 | less flexible | |

**User's choice:** 按调用配置
**Notes:** 按调用注入可让单个测试精确表达「第 N 次验证 PIN 失败」等场景。

### 子决策 4-ii：是否记录调用序列

| Option | Description | Selected |
|--------|-------------|----------|
| 记录调用序列 | fake 记录调用顺序供测试断言 | ✓ |
| 不记录 | | |

**User's choice:** 记录调用序列
**Notes:** 阶段 2 验证 RES-03/RES-04（资源在会话结束与错误路径上被释放）必须依赖调用序列断言，因此在阶段 1 建立该能力。

---

## the agent's Discretion

- 新模块的确切文件名与目录嵌套层级
- trait 方法的具体命名与签名细节
- fixture 测试的辅助函数与测试组织方式
- `provenance` 字段的具体编码格式
- fake provider 的内部数据结构与调用记录格式
- 纯函数提取时是否顺带补充文档注释

## Deferred Ideas

- 协议层抽取 → 阶段 2
- 领域层抽取（37 分支）→ 阶段 2，与会话改造合并
- 会话注册表与授权隔离 → 阶段 2
- 不透明资源标识与确定性释放 → 阶段 2
- 传输限制、超时、阻塞 FFI 隔离、按设备串行化 → 阶段 3
- 格式化与 Clippy 归一化 → 阶段 4
- `tempfile` / `proptest` 引入 → 视阶段 1 实际需要
- 稳定错误码对外暴露与协议版本协商 → 阶段 6
- `IssueCertificate` 双证书正确性 → 不在任何阶段（保持 Mock）
- `serde_yaml` 替换 → 独立里程碑

## Compatibility Constraints Carried Forward

以下约束在讨论中确认延续，未产生新的决策记录：

- 环境变量 `SKF_CONFIG`、`SKF_WS_ADDR`、`SKF_HTTP_ADDR`、`RUST_LOG` 继续生效
- `config/skf.yaml` 与 `packaging/windows/skf-windows-x86.yaml` 继续可解析
- FFI 符号名 / C 布局 / 调用约定规范兼容
- `i686-pc-windows-gnu` 编译检查与 `i686-pc-windows-msvc` 打包继续通过，产物名 `skf-service.exe` 不变
- 不引入重量级新依赖

---

*Phase: 01-structural-foundation-and-test-seam*
*Discussion logged: 2026-09-11*

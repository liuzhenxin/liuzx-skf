# Phase 3: Transport Hardening and Concurrency - Context

**Gathered:** 2026-09-11
**Status:** Ready for planning

<domain>
## Phase Boundary

让服务在异常输入、缓慢设备与并发客户端下保持有界行为，并把厂商 DLL 的阻塞与线程安全风险限制在受控边界内。

具体交付：

1. **帧/载荷边界**（TRANS-01）：WebSocket 帧与操作载荷在分配前被拒绝。
2. **连接数上界**（TRANS-02）：并发客户端连接数有上界，超限连接被明确拒绝。
3. **阻塞调用的运行时隔离**（TRANS-04）：厂商 FFI 不在 async 运行时 worker 上执行。
4. **有界/可取消的长操作**（TRANS-03）：缓慢设备不让无关客户端停摆；`WaitForDevEvent` 保持可取消。
5. **按设备/库串行化**（TRANS-05）：同一厂商库的原生调用被串行化，杜绝并发使用同一句柄。
6. **非回环绑定的显式 opt-in**（TRANS-06）：默认拒绝非回环绑定。
7. **破坏性操作分类**（TRANS-07）：只读与破坏性操作有文档化分类。

**本阶段不做**：格式化/lint 归一化（阶段 4）；Windows 服务就绪（阶段 5）；日志轮转、脱敏诊断、协议版本协商（阶段 6）；把 HTTP demo 也纳入回环限制（见 Deferred）。

**硬约束**：37 个 v0.2.0 夹具必须继续匹配；任何响应漂移都是契约变更，不得靠放宽 `normalize` 掩盖。

</domain>

<decisions>
## Implementation Decisions

### 原生调用串行化（TRANS-05，区域 2）
- **D-01:** 采用**每 provider 库一把全局锁**（2.1b），而非每设备锁。理由：厂商 DLL 线程安全性未知；当前部署以单把 GM3000 为主，全局锁的并发代价可接受，且失败模式远少于细粒度锁。将来若拿到 DLL 线程安全证据，再放宽为每设备锁是向后兼容的演进。
- **D-02:** 锁**必须显式豁免** `WaitForDevEvent` / `CancelWaitForDevEvent`。这两个调用是进程级、不带设备句柄，且设计要求二者并发——cancel 必须在 wait 阻塞期间到达。若纳入全局锁，cancel 会排在 wait 之后，直接死锁（Phase 1 已证实同类问题）。
- **D-03:** 锁放在 **native 内部**（2.2a）：`NativeSkfProvider` 持有 `Arc<Mutex<()>>`，`NativeDevice` / `NativeApplication` / `NativeContainer` / `NativeDigest` 各自 clone 该 `Arc`，在每个进入 FFI 的方法取锁。provider 顶层 `wait_for_event` / `cancel_wait_for_event` 不取锁。调用方（domain / 守卫 trait）对锁无感。
- **D-04:** **不区分只读与破坏性**（2.3a）：`enum_dev`、`get_dev_state` 等只读调用同样串行。理由：DLL 非线程安全时只读也可能改内部状态；区分会以正确性换未经验证的吞吐。
- **D-05:** 锁实现为 `std::sync::Mutex<()>` + 中毒恢复（`lock().unwrap_or_else(|e| e.into_inner())`）（2.4a）。native 调用在阻塞上下文执行、不跨 `.await` 持锁，`std` 锁正确且比 `tokio::sync::Mutex` 便宜；中毒恢复避免一次 panic 后全部请求永久失败，且不新增依赖。
- **D-06（落地顺序，不可违反）:** 锁**必须与 `spawn_blocking` 同批落地**。若先在 async worker 上引入 `std` 锁，同库并发会把 worker 线程卡在锁上，反而放大问题。

### FFI 隔离与超时（TRANS-04 / TRANS-03，区域 1）
- **D-07:** 采用**每个 FFI 调用点 `tokio::task::spawn_blocking`**（1.1a），延续 `WaitForDevEvent` 已验证的模式，依赖 Tokio 阻塞池。守卫是 `Send`，用"移入闭包再移出"的小辅助/宏处理守卫生命周期。不引入自建线程池或 actor（留待阶段 6，若需要更严线程上界）。
- **D-08:** 除 `WaitForDevEvent`/`CancelWaitForDevEvent` 外的 FFI 调用加 **30 秒超时**（1.2a）。超时是"调用方止损"，**不是取消设备操作**：厂商线程会继续执行到返回，锁亦随之释放。必须文档化这一区别。
- **D-09:** 超时返回 **`-1` + 明确消息**（1.3a，例如 `Device operation timed out after 30s`），并记录不含敏感数据的日志。夹具不覆盖挂起路径，不触发契约漂移。不新增客户端可见错误码。
- **D-10:** "不拖累无关客户端"的**确切边界**：async 运行时与所有不碰 token 的请求（`SetLanguage`、`EnumProvider` 等）不受影响；**同一 provider 库**的其它 token 请求会排队；不同 provider（不同 DLL）互不影响。这是全库锁下的可达保证，须写入文档与测试命名。
- **D-11:** 厂商调用无法被强杀，因此 TRANS-03 的"可取消"实际只由 `WaitForDevEvent`/`CancelWaitForDevEvent` 这一对 + 调用方超时共同满足；不要声称能中止已在执行的 FFI。

### 边界数值（TRANS-01 / TRANS-02 / TRANS-03，区域 3）
- **D-12:** WebSocket **帧/消息上限 1 MiB**（3.1a），通过 `tokio_tungstenite::accept_async_with_config` + `WebSocketConfig{max_message_size, max_frame_size}` 在 tungstenite 分配前拦截。
- **D-13:** **操作载荷上限 256 KiB（解码后）**（3.2a），在 `Params::required_base64` / `optional_base64` 中于 **base64 decode 之前** 校验编码串长度（`encoded.len() <= limit * 4 / 3 + 4`），超限返回 `-2` 并给出明确消息，不 panic、不先分配。
- **D-14:** **最大并发连接数 64**（3.3a）。accept 循环持 `tokio::sync::Semaphore(64)`，用 `try_acquire`；失败则记录日志并丢弃该 TCP 连接（不进入 WebSocket 握手）。不排队等待。
- **D-15:** 30 秒超时**仅覆盖 provider FFI**（3.4a）：device/application/container/digest 的所有方法 + 枚举/状态查询。`WaitForDevEvent`/`CancelWaitForDevEvent` 豁免。`IssueCertificate` 的 OpenSSL 子进程**不在本阶段覆盖**，作为已知无界路径记录（见 Deferred）。

### 非回环绑定 opt-in（TRANS-06，区域 4）
- **D-16:** opt-in 同时支持 **YAML 具名顶层键 `allow_remote: true`** 与 **环境变量 `SKF_ALLOW_REMOTE=1`**（4.1c），**任一为真即 opt-in**。YAML 作为可审计的持久开关；环境变量供运维/CI/Windows SCM 覆盖。为 `SkfConfig` 增加具名字段不违反 D-07：D-07 的问题是**未命名**嵌套键被 `#[serde(flatten)]` 捕获为 provider；具名字段会被正确绑定。
- **D-17:** **启动即拒绝**（4.2a）：在 `bind` 之前校验地址，非回环且未 opt-in 时返回错误、进程非零退出，消息明确指出 `allow_remote` / `SKF_ALLOW_REMOTE`。不做"回落到回环 + 警告"。
- **D-18:** 范围**仅 WebSocket 监听器**（4.3a）。`RunMode::Console` 下 HTTP demo 的既有默认 `0.0.0.0:8000` 保持不变，作为已知边界记录（见 Deferred）。
- **D-19:** 回环判定基于**解析后的 `SocketAddr`**（4.4a）：主机名先 `to_socket_addrs` 解析，再用 `Ipv4Addr::is_loopback()` / `Ipv6Addr::is_loopback()`；`0.0.0.0` 与 `::` 一律视为非回环。不按字面量匹配 `127.0.0.1`/`localhost`（会漏掉 `127.0.0.2`、`::1`、`0.0.0.0`）。

### 破坏性操作分类（TRANS-07，区域 5）
- **D-20:** 分类以**代码为真源**：定义 `pub enum OperationClass { ReadOnly, Destructive }` 并为全部 37 个方法建立显式映射。
  - `ReadOnly`：`SetLanguage`、`EnumProvider`、`EnumDevice`、`EnumApplication`、`EnumContainer`、`GetDevState`、`GetDevInfo`、`GetContainerType`、`FindCertificates`、`DigestInit`/`DigestUpdate`/`DigestFinal`/`CloseHash`、`Digest`、`SignData`、`RSASignData`、`ECCVerify`、`RSAVerify`、`EncryptData`、`DecryptData`、`GenerateRandom`、`WaitForDevEvent`、`CancelWaitForDevEvent`
  - `Destructive`：`ConnectDev`、`DisConnectDev`、`CheckPIN`、`CreateContainer`、`DeleteContainer`、`ImportCertificate`、`ImportKeyPair`、`CreatePKCS10`、`GenECCKeyPair`、`GenRSAKeyPair`、`LockDev`、`UnlockDev`、`SetLabel`、`Transmit`、`IssueCertificate`
  - 归类原则：任何改变设备/令牌持久状态、消耗重试计数、生成或导入密钥材料、打开/关闭原生资源、改变锁状态的操作均为 `Destructive`；纯查询与无状态计算为 `ReadOnly`。`SignData`/`EncryptData` 不改变持久状态，归 `ReadOnly`。
- **D-21:** 用一条不变量测试断言 `EXPECTED_METHODS` 中每个方法都有且仅有一个分类（防止新增方法漏分类）；分类表与 `EXPECTED_METHODS` 同源维护。
- **D-22:** 本阶段**不据分类做强制**（不做授权、审计或确认弹窗），只做分类与文档；文档表格引用该枚举。未来阶段（阶段 6 诊断/审计，或引入危险操作确认时）可据此强制。

### the agent's Discretion
- 锁辅助函数与 `spawn_blocking` 守卫搬迁辅助的具体形态与命名（建议集中在 `src/provider/native.rs` 与一个小工具模块）。
- `OperationClass` 的模块位置与映射数据结构的内部形态。
- 连接信号量、帧配置、载荷校验的具体接入点与错误消息措辞（D-09 的 `-1` 消息除外需明确）。
- 超时实现细节（`tokio::time::timeout` 包裹 `JoinHandle`，超时后忽略 JoinHandle 结果）。

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### 阶段范围与验收标准
- `.planning/ROADMAP.md` §Phase 3 — 目标、依赖、5 条成功标准
- `.planning/REQUIREMENTS.md` §Transport and Concurrency — TRANS-01..07 是本阶段全部需求
- `.planning/PROJECT.md` — 里程碑目标、Constraints（回环默认、仅一处 unsafe Send/Sync、i686 兼容）

### 阶段 2 产出（本阶段直接依赖）
- `.planning/phases/02-session-authorization-and-resource-ownership/02-CONTEXT.md` — D-07（配置 flatten 陷阱，决定 opt-in 走具名字段/环境变量）、D-09（不做后台设备事件广播）
- `.planning/phases/02-session-authorization-and-resource-ownership/02-RESEARCH.md` — §Verified Defects V-2/V-3、§Methodology Note（指标会说谎）
- `.planning/phases/02-session-authorization-and-resource-ownership/02-VERIFICATION.md` — 阶段 2 已验证状态
- `.planning/phases/02-session-authorization-and-resource-ownership/02-04-SUMMARY.md` — 未迁移分支与 provider 现状

### 阶段 1 产出（WaitForDevEvent 约束来源）
- `.planning/phases/01-structural-foundation-and-test-seam/01-VERIFICATION.md` — WaitForDevEvent 必须在阻塞池、cancel 并发到达的结论
- `.planning/phases/01-structural-foundation-and-test-seam/01-CONTEXT.md` — provider 边界与 fake 设计

### 现有源码（实现落点）
- `src/server/mod.rs` — `spawn_client`（当前无连接/帧限制）、`ServerOptions::from_env`、`bind`、`serve`
- `src/provider/mod.rs` — `SkfProvider` 契约、`Operation`、守卫 trait；"serialisation ... is the implementation's responsibility"
- `src/provider/native.rs` — 唯一允许命名原生句柄的文件；单库实例；将承载锁与 `spawn_blocking`
- `src/provider/fake.rs` — `fail_next` 与调用记录；并发/超时测试的注入点
- `src/domain/` — 12 个迁移后的处理器；`spawn_blocking` 的主要调用方
- `src/main.rs` — 14 个未迁移分支、`WaitForDevEvent` 的既有 `spawn_blocking` 模式
- `src/skf/types.rs` — 唯一的 `unsafe impl Send/Sync`
- `tests/providers_invariants.rs` / `tests/session_invariants.rs` — 不变量测试的写法
- `tests/common/mod.rs` 与 `tests/contract_fixtures.rs` — 端到端重放与子进程服务启动辅助（连接/帧限制测试可复用）

### 外部/兼容基线
- `tests/fixtures/v0.2.0/README.md` — 两条夹具规则、`normalize` 冻结、Contract Boundary
- `api/skf_api.js` — v0.2.0 客户端，参数顺序与响应形状兼容基线

**本项目无外部规格或 ADR 文档** —— 需求与约束完整记录于上述 `.planning/` 文件与源码本身。

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **`spawn_blocking` 既有模式**（`src/main.rs:340/365`）：`WaitForDevEvent` 已把阻塞 provider 调用放入阻塞池，并证明这不破坏 cancel 语义。新的 FFI 隔离应复用同一模式。
- **守卫体系**（`DeviceGuard`/`ApplicationGuard`/`ContainerGuard`/`DigestGuard`）：全部是 `Send`，因此可以把守卫 move 进 `spawn_blocking` 再 move 出。锁可直接挂在 `NativeSkfProvider` 并由守卫共享。
- **`FakeSkfProvider::fail_next` 与调用记录**：并发/超时测试的注入与断言基础已就绪；`Blocking(Duration)` 失败类型正是为"缓慢设备不阻塞他人"设计的。
- **`tests/contract_fixtures.rs::start_service`**：已能以临时端口启动真实二进制并读取绑定地址，连接数/帧限制的端到端测试可直接复用。
- **`ServerOptions::for_test`**：测试用的临时端口、无 HTTP 配置，适合回环 opt-in 测试。

### Established Patterns
- 错误处理：启动用 `anyhow` + context 并 `?` 传播；请求边界用 `RpcResponse::err(code, msg, id)` 早返回。
- 原生类型隔离：`src/provider/mod.rs` 不得出现原生句柄类型（`tests/provider_invariants.rs` 强制）；锁必须用整数/`Arc` 表达，不得引入新的 `unsafe impl`。
- 契约纪律：`normalize` 录制时冻结；任何响应漂移必须显式声明为契约变更。
- 配置：`SkfConfig` 的 `#[serde(flatten)]` 使**未命名**顶层键被当作 provider；新增配置一律用具名字段或环境变量。

### Integration Points
- `src/server/mod.rs::spawn_client` 是连接数信号量、帧配置（`accept_async_with_config`）的唯一落点。
- `bind` 是回环校验与拒绝的落点（在 `TcpListener::bind` 之前）。
- `src/provider/native.rs` 是锁与 `spawn_blocking` 的共同落点；任何 native 方法都必须经锁。
- `src/protocol/params.rs` 是载荷上限校验的落点（decode 之前）。
- `src/domain/mod.rs` 或新 `classification.rs` 是 `OperationClass` 与 37 方法映射的落点。

</code_context>

<specifics>
## Specific Ideas

- 用户明确选择**全库全局锁而非每设备锁**，接受单 provider 内串行，理由是 DLL 线程安全性未知，且将来可向后兼容地放宽。
- 用户明确要求把 `WaitForDevEvent`/`CancelWaitForDevEvent` 作为**豁免对**处理，避免 cancel 被 wait 阻塞而死锁。
- 用户接受"超时只止损、不取消设备操作"这一现实，并要求文档明确边界。
- 用户要求 opt-in **既可审计（YAML）又便于运维（环境变量）**，任一即生效。
- 用户明确接受 **HTTP demo 保持非回环默认**作为本阶段边界，不纳入 TRANS-06。
- 用户接受破坏性操作分类**本阶段只分类与文档、不强制**。

</specifics>

<deferred>
## Deferred Ideas

- **HTTP demo 的非回环默认**（`RunMode::Console` 的 `0.0.0.0:8000`）→ 本阶段明确排除；若将来把静态 demo 也视为攻击面，单独处理。
- **`IssueCertificate` 的 OpenSSL 子进程无界** → 本阶段不覆盖，记录为已知限制；阶段 6 或废弃该 Mock 能力时处理。
- **每设备细粒度并行** → 待取得厂商 DLL 线程安全证据后再评估（当前 D-01 的全局锁是防御性默认）。
- **自建固定大小阻塞池 / FFI actor** → 若阶段 6 证明 Tokio 阻塞池成为瓶颈或需要更严线程上界，再替换 D-07。
- **后台 `WaitForDevEvent` 实时失效广播** → 阶段 2 D-09 已推迟；本阶段仅保证取消语义。
- **据 `OperationClass` 强制授权/审计/确认** → 本阶段只分类；强制留待阶段 6。
- **`provider_load_failed` 消息中暴露库路径**（阶段 2 L-05）→ 阶段 6 脱敏工作时处理。
- **固定 64 KiB 缓冲区与 `wait_for_event` 入参长度**（阶段 1 L-03/M-02）→ 本阶段的载荷边界工作应一并核对。

</deferred>

---

*Phase: 03-transport-hardening-and-concurrency*
*Context gathered: 2026-09-11*

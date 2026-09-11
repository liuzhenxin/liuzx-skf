# Phase 2: Session Authorization and Resource Ownership - Research

**Researched:** 2026-09-11
**Domain:** Session-scoped authorization and native resource ownership in a Rust FFI gateway
**Confidence:** HIGH (every finding below is measured or read from this repository; no external claims)

<user_constraints>
## User Constraints (from CONTEXT.md)

**CRITICAL:** These are locked decisions from `02-CONTEXT.md`. They MUST be honored by the planner.

### Locked Decisions
- **D-01:** 注册表 + 会话 ID（方案 1A）。
- **D-02:** 会话 ID 为 32 位十六进制，由 `OsRng` 生成。
- **D-03:** 会话 ID 不下发给客户端。
- **D-04:** 不允许客户端重连续用同一会话（与 SESS-04 冲突，明确排除）。
- **D-04b:** 注册表不得变成"换了个名字的同一缺陷"：领域层只能访问自己会话的状态；跨会话访问仅允许出现在设备拔出失效这一条路径。
- **D-05:** 授权表示为 `(provider, device, app) -> expires_at`。
- **D-06/D-07:** TTL 来自环境变量 `SKF_AUTH_TTL_SECONDS`，默认 600 秒；**不得**写入 `config/skf.yaml`（`#[serde(flatten)]` 会让整份配置解析失败）。
- **D-08:** `SKF_AUTH_TTL_SECONDS=0` 不表示"永不超时"；无效值回落默认并记录警告。
- **D-09:** 清除触发为断连、超时、操作时检测到设备不可用；不做后台 `WaitForDevEvent` 广播。
- **D-10:** PIN 验证后不保留任何可恢复副本。
- **D-11:** 句柄形态为会话内计数器 + 类型前缀（`dev-1`、`app-1`、`cnt-1`、`hsh-1`）。
- **D-12:** 未知/过期/类型错误统一返回 `-11`，按原因区分消息。
- **D-13:** digest 路径的消息文本是冻结契约；改动须按契约变更流程处理。
- **D-14:** 句柄前缀不出现在错误消息中。
- **D-15:** 句柄在会话结束时全部失效并释放；跨会话引用必须被拒绝。
- **D-16:** 迁移 19 个安全相关分支（清单见 D-16）。
- **D-17:** 其余 14 个只读分支保持直连至阶段 3。
- **D-18:** 已路由的 4 个分支保持，但改为经会话解析设备名。
- **D-19:** 协议层抽取到 `protocol/`，参数解析统一为类型化辅助。
- **D-20:** 领域层抽取到 `domain/`（D-16 的 19 个分支）。
- **D-21:** 未迁移的 14 个分支不得夹带行为变更。
- **D-22:** 全 crate 仅一处 `unsafe impl Send/Sync`；守卫以整数存储句柄。
- **D-23:** SESS-02 必须用对抗性测试验证（两会话）。
- **D-24:** 四个 `SKF_*` 环境变量继续生效；两份 YAML 结构与语义不变。
- **D-25:** Windows i686 构建与 v0.2.0 客户端契约继续成立。

### the agent's Discretion
- `protocol/`、`domain/`、`session/` 内部文件划分与嵌套
- 类型化参数提取辅助的形态与命名
- 会话注册表的内部数据结构与清理时机
- 错误消息措辞（digest 路径除外）
- 是否记录会话计数到诊断

### Deferred Ideas (OUT OF SCOPE)
- 传输限制、连接数上限、帧大小检查 → 阶段 3
- 阻塞 FFI 全面隔离、按设备串行化 → 阶段 3
- 其余 14 个只读分支迁移 → 阶段 3
- `provider_load_failed` 消息中的库路径（L-05）、`UnavailableProvider` 的 `path` 字段（L-01）→ 阶段 6
- 固定 64 KiB 缓冲（L-03）与 `wait_for_event` 入参长度（M-02）→ 阶段 3
- `EXPECTED_METHODS` 手工维护（L-06）→ 下次新增方法时
- 后台实时失效广播（D-09 已定不做）
- `serde_yaml` 替换 → 独立里程碑

</user_constraints>

<architectural_responsibility_map>
## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| 会话身份与注册表 | Library crate — `session/` | — | 跨连接共享，需要锁；但不参与请求语义 |
| 授权状态与过期 | Library crate — `session/` | `domain/` 只读查询 | 会话拥有，领域层只问不拥有 |
| 协议解析与参数提取 | Library crate — `protocol/` | — | 无状态、纯函数、可零依赖单测 |
| 方法语义（19 个分支） | Library crate — `domain/` | `provider` | 组合会话 + provider |
| 传输与连接生命周期 | Library crate — `server/` | — | 已建立；只在会话创建/销毁处接线 |
| 组合根与 `Session` 实现 | Binary crate — `main.rs` | — | 依 D-06/D-20，派发器本次迁出，但组合根保留 |
| 未迁移的 14 个分支 | Binary crate — `main.rs` | — | D-21：保持原样，不得夹带变更 |

</architectural_responsibility_map>

<research_summary>
## Summary

本阶段要动的代码已经量出来了：**19 个分支、71 处位置参数提取、12 处 `ctx.pins` 读取加 1 处写入、4 处 `hash_handles` 访问、246 处 `return RpcResponse::err`**。研究把"要改多少"变成了具体数字，也把"风险在哪"变成了三条可证实的缺陷。

**最重要的发现是会话状态在接缝上是空的。** `JsonRpcSession { ctx, lang }` 没有任何会话级状态，也没有 `Drop`；`pins` 与 `hash_handles` 都是 `SkfContext` 上的进程级字段。这意味着"把状态迁到会话"不是微调，而是把两处进程级 map 拆成每连接一份。

**第二条是 digest 句柄会永久泄漏设备句柄。** `hash_handles` 只在 `DigestFinal` 与 `CloseHash` 中被 `remove`（3364 插入、3414 与 3453 移除），没有任何断连清理路径。客户端调用 `DigestInit` 后直接断开，该设备句柄就永远留在进程级 map 里——这正是 RES-03/RES-05 要消除的行为，且可由代码直接证明。

**第三条是契约边界比担心的窄。** 我逐条核对了 4 个 digest 夹具：`DigestInit` 录到的是**错误响应**（`ConnectDev failed`），因此成功形态 `{"handle": "hash_N"}` **不在冻结范围内**，句柄格式可以自由更改；而 `DigestUpdate`/`DigestFinal`/`CloseHash` 冻结的是对输入 `"hash_1"` 的**拒绝**契约（`-11` + 精确消息文本）。结论：格式可改，拒绝契约须保留——这比走契约变更流程便宜且能保住 oracle。

**一条方法论警告也记录在案。** 我最初用"open/close 计数不平衡 + return 时仍有未关闭资源"的启发式扫出 26 个疑似泄漏点，随后核对 `DeleteContainer` 发现它的 8 个 return **全部正确清理**——该指标把"open 失败后立即返回"也算成了泄漏。**这个指标不能作为 RES-04 范围的证据**，泄漏必须逐分支确认。

**Primary recommendation:** 计划按 `会话骨架（注册表 + Drop 生命周期）→ 授权（TTL + 清除触发）→ 不透明句柄 → 协议层 → 19 分支迁移 → 端到端验证` 组织。前两步为后续提供基座，句柄与迁移必须在其上做，否则会重写两次。
</research_summary>

<session_state_inventory>
## Measured Session-State Surface

数据来源：对 `src/main.rs` 的静态分析（本次研究执行）。

| 项目 | 数量 | 位置 |
|------|------|------|
| `ctx.pins` 读取 | 12 | 行 625, 1043, 1146, 1313, 1755, 2023, 2204, 2612, 2788, 3053, 3162（+1 未列出为 write） |
| `ctx.pins` 写入 | 1 | 行 1684（`CheckPIN` 成功后插入明文 PIN） |
| `ctx.hash_handles` 写入 | 2 | 行 3363（`DigestInit` insert）、3413/3452（remove 前的 write 锁） |
| `ctx.hash_handles` 读取 | 1 | 行 3386（`DigestUpdate`） |
| `ctx.hash_handles` 移除 | 2 | 行 3414（`DigestFinal`）、3453（`CloseHash`） |
| `JsonRpcSession` 字段 | 2 | `ctx`、`lang`；**无 `Drop`、无会话级状态** |
| `return RpcResponse::err` | 246 | 全文件 |
| `RpcResponse::err` | 274 | 全文件 |
| 19 个目标分支的 `req.params.get(` | **71** | 单分支最多 10（`SignData`） |
| `skf_service::provider` 在 main.rs 出现 | 1 | 生产路径几乎未使用 provider |
| provider 守卫使用 | 2 | 同上 |

**推论：** 会话状态目前**没有任何**会话级载体；迁移不是"把 map 加到会话上"，而是"把两处进程级 map 拆成每连接一份，并让 19 个分支改从会话取"。
</session_state_inventory>

<verified_defects>
## Verified Defects (evidence-backed)

### V-1 — 被放弃的 digest 句柄永久泄漏设备句柄

`hash_handles` 的全部访问点：

```
3363  hash_handles.write()   // DigestInit: insert(handle_key, (h_hash, h_dev, provider, lib_path))
3386  hash_handles.read()    // DigestUpdate
3414  hash_handles.remove()  // DigestFinal
3453  hash_handles.remove()  // CloseHash
```

**没有任何断连清理路径**：`JsonRpcSession` 未实现 `Drop`，`server::spawn_client` 在连接结束时只是 drop 掉 session 对象。因此 `DigestInit` 之后直接断开，会把一个**已连接的设备句柄**永久留在进程级 map 中。

**影响：** SESS-04、RES-03、RES-05 的直接来源。也是"注册表 + 会话级句柄表"能解决的核心问题。

**验证方式：** 阶段 2 的测试应为「`DigestInit` → 断连 → 断言设备被断开」，今天该测试必然失败。

### V-2 — 6 个目标分支按次加载并释放厂商库

`Library::new` 在以下目标分支内出现 1 次：`GenECCKeyPair`、`GenRSAKeyPair`、`DigestInit`、`DigestUpdate`、`DigestFinal`、`CloseHash`。

与 V-1 组合后更严重：`DigestInit` 用库实例 L1 打开设备并保存句柄；`DigestUpdate` 构造**新的**库实例 L2，并把 L1 产出的句柄交给 L2。在 Windows 上这意味着 `FreeLibrary` 语义可能使句柄失效。

**影响：** M-03（code review 记录）的具体化；迁移这 6 个分支时路由到 provider 即可消除。

### V-3 — `CString::into_raw()` 泄漏 69 处

`grep -c 'into_raw()' src/main.rs` → **69**。

`CString::into_raw()` 交出所有权且永不回收。每个请求路径都在泄漏一小块内存。Phase 1 写的 `provider/native.rs` 已经用 `as_ptr() as *mut CHAR` 避免该问题，因此迁移会顺带修掉这 69 处。

**注意：** 这属于"迁移顺带修复"，不应作为独立任务，否则会把无关改动混入契约验证。

</verified_defects>

<contract_boundary>
## The Digest Contract Boundary — Narrower Than Assumed

逐条读取 4 个 digest 夹具后的结论：

| 夹具 | 冻结内容 | 对 Phase 2 的含义 |
|------|----------|-------------------|
| `DigestInit` | `{"error": 167772195, "message": "ConnectDev failed: 0x0A000023"}` | **成功形态未冻结**。`{"handle": "hash_N"}` 不在任何夹具中 → **句柄格式可以自由更改** |
| `DigestUpdate` | request `params[0] = "hash_1"`；response `{"error": -11, "message": "Invalid or expired hash handle"}` | 对未知句柄的**拒绝契约被冻结** |
| `DigestFinal` | 同上（`"hash_1"` → `-11` + 同一消息） | 同上 |
| `CloseHash` | 同上 | 同上 |

**结论与建议：**

1. 句柄的**内部格式**从 `hash_{jsonrpc_id}` 改为 `hsh-{n}` **不破坏任何夹具**（因为成功形态未被录制）。
2. `DigestUpdate`/`DigestFinal`/`CloseHash` 对 `"hash_1"` 的拒绝必须继续返回 **`-11` + `"Invalid or expired hash handle"`**。这与 D-13 一致，且**比走契约变更流程便宜**——该消息不泄露句柄格式（D-14 也要求不泄露），因此保留它没有代价。
3. 因此本阶段**不需要**契约变更。计划应把"digest 拒绝消息文本保持不变"写进对应任务的验收标准。

</contract_boundary>

<design_guidance>
## Design Guidance Derived From the Code

### G-1 — 会话状态不需要任何内部可变性

`Session::handle` 的签名是 `fn handle<'a>(&'a mut self, text: &'a str) -> BoxFuture<'a, String>`（Phase 1 建立）。它给出 `&mut self`，而守卫类型是 `Send` 但**不是** `Sync`（`Box<dyn DeviceGuard>` 等）。

推论：**一个连接 = 一个会话 = 独占访问**。会话自己拥有 `HashMap<String, Box<dyn XxxGuard>>` 即可，**不需要 `Mutex`/`RwLock`**。需要锁的只有跨连接共享的注册表。这比"给会话状态加锁"简单得多，也消除了嵌套锁的风险。

### G-1b — 注册表只能引用轻量 marker，不能引用会话状态

会话状态必然包含 `Box<dyn DeviceGuard>` 等 provider 守卫，它们是 `Send` 而**非** `Sync`（守卫 trait 只要求 `Send`）。因此：

- `Arc<SessionState>` **不是 `Send`**，会话就无法移入 Tokio 任务；
- 若为了让 `Arc<SessionState>` 变成 `Send + Sync` 而加 `unsafe impl Sync for SessionState`，会违反 D-22（全 crate 仅一处 `unsafe impl Send/Sync`）。

**结论：** 注册表存 `Weak<SessionMarker>`（`SessionMarker` 只含 `SessionId`，天然 `Send + Sync`），`SessionState` 由 `SessionGuard` **按值**持有。这样 `SessionGuard: Send` 成立、会话可移入任务、且不需要任何新的 `unsafe impl`。

**这条推论必须由测试固化**：`assert_send::<SessionGuard>()`。否则将来有人把状态改回 `Arc` 会静默破坏可移动性。

### G-2 — 注册表的接口必须窄

D-04b 要求它不能成为新的全局缺陷。建议只暴露两个操作：

- `register()`（工厂创建会话时）与随 `SessionGuard` drop 的注销
- `session_count() -> usize`（仅诊断计数）

**不要**提供 `get(id)`、"列出所有会话"，**也不要**提供任何跨会话修改入口。

**为什么没有 `invalidate_for_device`（设计修订）：** 曾考虑用它把"设备拔出"广播到受影响会话，但那需要注册表持有 `Weak<SessionState>` 后修改其内部状态，从而给会话状态引入 `Mutex` —— 与 G-1（`&mut self` 已足够）冲突，并把注册表变成"换名字的共享状态缺陷"（D-04b）。改为**由观察到失败的会话自己清除自己的授权条目**：会话在该路径上持有 `&mut`，无需锁，注册表保持无副作用。代价是失效为"下次使用时发现"而非实时——这正是 D-09 已选择的取舍。

### G-3 — 句柄表的类型编码

`dev-N`/`app-N`/`cnt-N`/`hsh-N` 前缀使"类型错误"可判定：解析前缀后查对应子表，前缀不符即 `-11`（D-12）。由于会话独占，子表可以是同一张 `HashMap<String, SessionResource>`，其中 `SessionResource` 是枚举而非 `Any`，避免运行期向下转型。

### G-4 — 授权表与句柄表的键不同，不要合并

授权键是 `(provider, device, app)`；句柄键是会话内不透明 id。两者生命周期不同：授权可过期（TTL），句柄只随会话结束失效。合并会让"授权过期"意外释放句柄或反之。

### G-5 — `wait_for_event` 的进程级语义是会话隔离的例外

`SKF_CancelWaitForDevEvent` 是**进程全局**的，`skf_api.rs` 的 `cancel_wait_for_dev_event(&self)` 不接受句柄。因此两个会话同时等待时，任一方 cancel 都会影响另一方——**这个操作在协议层面无法做到会话隔离**。

建议：不要假装它被隔离。计划应显式记录该例外，并（可选地）在会话级串行化等待，使"同时只有一个会话在等"。这属于设计选择，需要计划明确表态。

### G-6 — TTL 的读取位置

`build_sessions` 已接收 `&SkfConfig` 与 `&Arc<dyn SkfProvider>`，是读取 `SKF_AUTH_TTL_SECONDS` 并注入注册表的自然位置。启动时若值无效应记录警告并回落默认（D-08）。**不要**在每次请求时读环境变量。
</design_guidance>

<architecture_patterns>
## Architecture Patterns

### 目标数据流

```
TCP accept
   │
   ▼
server::spawn_client ──► SessionFactory::create()
   │                          │
   │                          ├─ OsRng → SessionId (32 hex)   [D-02]
   │                          ├─ registry.register(id)          [D-01]
   │                          └─ SessionState { auth: {}, handles: {}, lang }
   │
   ▼
loop over text frames
   │
   ├─► Session::handle(&mut self, text)          ← &mut → 无需给会话状态加锁  [G-1]
   │        │
   │        ├─ protocol::parse(text) → typed request          [D-19]
   │        ├─ auth check: (provider, device, app) 是否未过期  [D-05]
   │        ├─ handle lookup: 会话内表，前缀判定类型            [G-3]
   │        └─ domain::<method>(&mut session, req)             [D-20]
   │                 └─ provider guard 在 Drop 中释放
   │
   ▼
Drop(JsonRpcSession)
   ├─ registry.deregister(id)                     [SESS-04]
   ├─ 释放全部会话句柄（守卫 Drop）                 [RES-03]
   └─ 授权条目随会话消失（不存 PIN）                [SESS-05]
```

### 推荐模块结构

```
src/
├── session/
│   ├── mod.rs        # SessionState, SessionId, 授权表, 句柄表
│   └── registry.rs   # SessionRegistry: register / deregister / invalidate_for_device
├── protocol/
│   ├── mod.rs        # RpcRequest, RpcResponse, Language
│   └── params.rs     # 类型化提取辅助（required_str / optional_str / required_u64 / base64 …）
├── domain/
│   ├── mod.rs        # 19 个分支的入口分派
│   ├── device.rs     # ConnectDev, DisConnectDev, GenerateRandom
│   ├── container.rs  # CreateContainer, DeleteContainer, GetContainerType, ImportCertificate
│   ├── crypto.rs     # SignData, RSASignData, CreatePKCS10, EncryptData, DecryptData
│   ├── keys.rs       # GenECCKeyPair, GenRSAKeyPair
│   ├── digest.rs     # DigestInit, DigestUpdate, DigestFinal, CloseHash
│   └── pin.rs        # CheckPIN
└── main.rs           # 组合根 + Session/SessionFactory 实现 + 14 个未迁移分支
```

### Anti-Patterns to Avoid

- **给会话状态加锁**：`&mut self` 已经给了独占访问（G-1）。加锁是纯粹的开销与死锁面。
- **让注册表提供 `get(id)`**：那是把进程级共享状态重新引入领域层（D-04b）。
- **合并授权表与句柄表**：生命周期不同（G-4）。
- **假装 `CancelWaitForDevEvent` 被会话隔离**：它是进程全局的（G-5）。
- **用 open/close 计数当作泄漏证据**：已验证该指标会误报（见 Methodology Note）。
- **为句柄格式变更走契约变更流程**：不需要——成功形态未被录制（Contract Boundary）。
</architecture_patterns>

<methodology_note>
## Methodology Note — a metric that lies

研究过程中我用「open/close 计数不平衡 + return 时仍有未关闭资源」的线性扫描，扫出 **26 个疑似泄漏点**。逐字核对 `DeleteContainer`（该启发式标记了 2 处）后发现它的 **8 个 return 全部正确清理**：

- `ConnectDev failed` 的返回发生在 `connect_dev` 失败时——没有资源需要释放
- `OpenApplication failed` 的返回**先调用了** `dis_connect_dev`
- PIN 失败与未登录两条路径都同时关闭了 application 与 device
- 成功路径在返回前关闭了两者

启发式的缺陷在于：它在 `open_*` **调用出现**时就递增计数，无法区分"调用失败所以无需释放"与"调用成功但忘记释放"。

**记录此事的用途：** 计划不得以该指标作为 RES-04 的范围依据。真实泄漏点须逐分支确认；已确认的可证实缺陷是 V-1（被放弃的 digest 句柄）与 V-3（69 处 `CString` 泄漏），两者都不依赖该启发式。
</methodology_note>

<common_pitfalls>
## Common Pitfalls

### Pitfall 1: 把进程级 map 拆到会话时，遗漏某条读取路径
**What goes wrong:** 19 个分支中 12 处读 `ctx.pins`、1 处写；漏掉任一处会留下"看起来仍能工作"的旁路，授权隔离被绕过。
**How to avoid:** 迁移完成后断言 `grep -c 'ctx\.pins' src/main.rs == 0` 且领域层不出现进程级 map；用两会话对抗性测试（D-23）证明隔离真实生效。
**Warning signs:** `ctx.pins` 仍出现在 `main.rs`；测试只覆盖单会话。

### Pitfall 2: 授权过期只实现"检查"而没实现"清除"
**What goes wrong:** 条目仍在表中，只是判定为过期。若句柄或 PIN 残留在条目里，SESS-05 不成立。
**How to avoid:** 条目**只**存 `expires_at`，不存 PIN；过期判定返回假并移除条目。
**Warning signs:** 授权结构体里有 `pin: String`。

### Pitfall 3: 会话结束时只注销注册表，没释放句柄
**What goes wrong:** 注册表干净了，但设备句柄仍被进程持有直到退出（V-1 的重演）。
**How to avoid:** 句柄表由会话拥有，`Drop` 时一并 drop；测试断言 `CloseDevice` 在断连后被记录一次。

### Pitfall 4: 用"计数不平衡"当作泄漏证据
**What goes wrong:** 26 个误报会让计划把范围扩大到一个不存在的问题上。
**How to avoid:** 见 Methodology Note；以可证实缺陷为准。

### Pitfall 5: 迁移 19 个分支时夹带行为变更
**What goes wrong:** 与 Phase 1 的教训同类——搬运中顺手"改进"错误消息或边界处理，使回归不可归因。
**How to avoid:** 每个分支迁移后用其夹具单独验证；`DigestUpdate`/`DigestFinal`/`CloseHash` 的拒绝消息文本必须逐字保留（Contract Boundary）。真正的行为改进（如 V-2/V-3）作为同一次迁移的**明确列出**的副作用，不隐藏。

### Pitfall 6: 设备拔出失效做成实时广播
**What goes wrong:** 引入常驻阻塞线程与跨会话广播路径，复杂化热路径（D-09 已决定不做）。
**How to avoid:** 操作时检测；注册表只提供 `invalidate_for_device`，供将来需要时调用。
</common_pitfalls>

<sota_updates>
## State of the Art

未做联网核实。与本阶段相关的取向：

| Old Approach | Current Approach | Impact |
|--------------|------------------|--------|
| 进程级凭据缓存 | 每会话授权 + TTL | 本阶段的核心变更 |
| 客户端传递原生句柄 | 服务端签发不透明 id | RES-01；本阶段落地 |
| 手工逐路径 close | RAII 守卫 | Phase 1 已建好守卫，本阶段开始使用 |
| 快照测试自动接受新基线 | 显式、需评审的契约差异 | 继续沿用；本阶段无需契约变更 |

</sota_updates>

<validation_architecture>
## Validation Architecture

Nyquist 验证门禁要求本节。计划阶段将据此产出 `02-VALIDATION.md` 的逐任务映射。

### Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust 内置 `cargo test`（`#[test]` / `#[tokio::test]`） |
| **Config file** | none — 复用 Phase 1 建立的 lib 目标与 `tests/common/mod.rs` |
| **Quick run command** | `cargo test --lib` |
| **Full suite command** | `cargo test` |
| **Estimated runtime** | < 60 s（新增两会话对抗性测试与断连释放测试） |

### Sampling Rate

- **After every task commit:** `cargo test --lib`
- **After every plan wave:** `cargo test` + `cargo check --target i686-pc-windows-gnu`
- **Before `$gsd-verify-work`:** full suite green
- **Max feedback latency:** ~60 s

### Wave 0 Requirements

- [ ] `src/session/mod.rs` + `src/session/registry.rs` —— 会话状态与注册表骨架
- [ ] `src/protocol/mod.rs` + `src/protocol/params.rs` —— 类型化参数提取
- [ ] `tests/session_isolation.rs` —— 两会话对抗性测试骨架（SESS-02）
- [ ] `tests/session_lifecycle.rs` —— 断连释放测试骨架（RES-03）
- [ ] `FakeSkfProvider` 的 `DeviceRemoved` 注入已就绪（Phase 1 已实现）

### Per-Task Verification Map（框架契约）

| Requirement | Automated Command | Verification Type |
|-------------|-------------------|-------------------|
| SESS-01 | `cargo test session` | 每连接会话 ID 唯一且不可预测（32 hex，两次不同） |
| SESS-02 | `cargo test --test session_isolation` | 对抗性：A 验 PIN 后 B 的敏感操作仍被拒 |
| SESS-03 | `cargo test session::auth` | 假时钟推进超过 TTL 后授权失效 |
| SESS-04 | `cargo test --test session_lifecycle` | 断连后授权不可用；设备移除后相关会话失效 |
| SESS-05 | `grep -c 'ctx\.pins' src/main.rs` == 0 且授权结构体无 PIN 字段 | 结构断言 |
| SESS-06 | `cargo test --lib` | 未授权会话返回稳定错误码 |
| RES-01 | `cargo test domain::device` | `ConnectDev` 返回 `dev-N` 形态；客户端整数不再被接受为句柄 |
| RES-02 | `cargo test session::handles` | 未知/过期/类型错误各自被拒 |
| RES-03 | `cargo test --test session_lifecycle` | 断连（含异常断开）后所有句柄释放 |
| RES-04 | `cargo test domain` | 原生错误路径同样释放（用 fake 注入失败后断言 `Close*` 被记录） |
| RES-05 | `cargo test --test session_isolation` | 会话 A 的 digest 句柄在 B 中不可用 |
| 契约 | `cargo test --test contract_fixtures` | 37 个夹具继续匹配；digest 拒绝消息逐字不变 |

### Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| 真实 GM3000 下的会话隔离 | SESS-02 增强 | 无设备时无法走成功路径 | 在有驱动的机器上：A 会话 `CheckPIN` 成功 → B 会话执行 `SignData` 必须被拒 |
| Windows i686 产物 | D-25 | macOS 无 MSVC | 触发 `release-windows.yml`，`build.ps1` 的 PE 断言即证据 |

### Validation Sign-Off

- [ ] 每个任务具备自动化验证命令或 Wave 0 依赖
- [ ] 采样连续性：不存在连续 3 个任务缺少自动化验证
- [ ] Wave 0 覆盖全部缺失引用
- [ ] 反馈延迟 < 60 s
- [ ] 计划完成后置 `nyquist_compliant: true`

</validation_architecture>

<open_questions>
## Open Questions

1. **`CancelWaitForDevEvent` 的会话隔离无法实现（G-5）**
   - What we know: `SKF_CancelWaitForDevEvent` 是进程全局的，不接受句柄；两会话同时等待时互相干扰。
   - What's unclear: 是否要在会话级串行化等待，还是接受该例外并文档化。
   - Recommendation: 计划应明确表态。建议**不**串行化（避免为罕见的并发等待引入阻塞队列），但把该例外写入 `domain/` 的文档注释与 Phase 6 的威胁模型。

2. **每个会话是否要限制并发 digest 数量**
   - What we know: V-1 让被放弃的 digest 句柄在本阶段后仍会占用设备资源，只是随会话结束释放。
   - What's unclear: 单个会话内 `DigestInit` 无上限是否构成资源耗尽风险。
   - Recommendation: 属阶段 3 的限制工作（TRANS-02），本阶段只在测试中记录该行为，不设上限。

3. **未迁移的 14 个分支会不会破坏会话语义**
   - What we know: 它们不接收句柄、不验证 PIN（D-17），因此不受授权表影响。
   - What's unclear: `FindCertificates` 与 `EnumApplication`/`EnumContainer` 会自行 `connect_dev`，是否应改由会话持有这些临时设备。
   - Recommendation: 本阶段不动它们（D-21）。但在 `domain/` 迁移时若发现它们读 `ctx.pins`，必须改为读会话——那属于"遗漏路径"（Pitfall 1），不是新范围。

4. **`skf_api.js` 是否需要更新**
   - What we know: D-12 复用 `-11`、D-15 拒绝跨会话句柄，响应形状不变。
   - What's unclear: 客户端是否需要感知"句柄属于会话"。
   - Recommendation: 不需要。客户端已把句柄当不透明字符串处理；本阶段不应修改 `api/skf_api.js`。
</open_questions>

<sources>
## Sources

### Primary (HIGH confidence) — 本仓库直接测量
- `src/main.rs` 静态分析：19 个分支的 `params.get` 共 71 处；`ctx.pins` 12 读 + 1 写；`hash_handles` 4 处访问；246 处 `return RpcResponse::err`；69 处 `into_raw()`；`skf_service::provider` 仅 1 处
- `src/main.rs` 行 3363/3386/3413/3452/3453 — `hash_handles` 全部访问点，证明无断连清理（V-1）
- `src/main.rs` 逐字读取 `DeleteContainer` — 8 个 return 全部正确清理（Methodology Note）
- `tests/fixtures/v0.2.0/{DigestInit,DigestUpdate,DigestFinal,CloseHash}.json` — 契约边界（`DigestInit` 成功形态未冻结；其余三条拒绝契约已冻结）
- `src/main.rs` `impl Session for JsonRpcSession` — `handle<'a>(&'a mut self, ...)`，`&mut` 使会话状态无需内部可变性（G-1）
- `src/provider/{mod,native,fake}.rs` — 守卫 `Drop`、`fail_next`、调用记录
- `src/server/mod.rs` — `SessionFactory::create` 与会话创建点
- `src/config/mod.rs` — `#[serde(flatten)] libs` 解析行为（D-07 来源）
- `src/skf/api.rs` — `cancel_wait_for_dev_event(&self)` 无句柄参数（G-5 来源）
- Phase 1 产出：`01-RESEARCH.md`、`01-VERIFICATION.md`、`01-REVIEW.md`、`01-REVIEW-FIX.md`、`tests/fixtures/v0.2.0/README.md`

### Secondary (MEDIUM confidence)
- 六条设计指引（G-1…G-6）由上述证据推导；每条都可追溯到具体代码事实。

### Tertiary (LOW confidence — needs validation)
- `wait_for_event` 在两会话并发时的具体干扰表现未被实测（缺设备）；计划中应作为设计选择而非实测结论陈述。

</sources>

<metadata>
## Metadata

**Research scope:**
- Core: 会话授权隔离与原生资源归属
- Ecosystem: 无新依赖（`rand` 已提供 `OsRng`）
- Patterns: 注册表 + 会话所有权、`&mut` 免锁、类型前缀句柄表、类型化参数提取
- Pitfalls: 遗漏状态路径、只检查不清除、只注销不释放、误用计数指标、夹带行为变更

**Confidence breakdown:**
- Standard stack: HIGH —— 结论是"不新增依赖"
- Architecture: HIGH —— 由 `Session::handle(&mut self)` 的签名与守卫的 `Send`-非-`Sync` 直接推得
- Pitfalls: HIGH —— 每条对应可复核的代码事实或已证伪的指标
- Contract boundary: HIGH —— 逐条读取 4 个夹具得出

**Research date:** 2026-09-11
**Valid until:** 阶段 2 结束（结论基于当前源码状态）
</metadata>

---

*Phase: 02-session-authorization-and-resource-ownership*
*Research completed: 2026-09-11*
*Ready for planning: yes*

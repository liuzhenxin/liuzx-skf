# Phase 2: Session Authorization and Resource Ownership - Context

**Gathered:** 2026-09-11
**Status:** Ready for planning

<domain>
## Phase Boundary

消除 Phase 1 研究确认的两个最严重缺陷，并处理 Phase 1 刻意推迟的两项结构抽取：

1. **授权跨连接共享** —— 当前 `SkfContext.pins` 以 `provider/dev/app` 为键进程级保存明文 PIN，任一连接验证过的授权对所有连接生效，且从不超时。
2. **客户端可控制原生句柄** —— `ConnectDev` 返回指针的十进制字符串，`DisConnectDev` 把它 `as DEVHANDLE` 直接转换后交给厂商库。伪造句柄已被证实能**杀死服务进程**。
3. **协议层抽取**（Phase 1 决策 D-07 推迟至本阶段）：`RpcRequest`/`RpcResponse`/`Language`/参数解析 → `protocol/`。
4. **领域层抽取**（Phase 1 决策 D-08 推迟至本阶段）：19 个安全相关方法分支 → `domain/`，并路由至 provider。

**本阶段不做**：传输限制、超时、阻塞 FFI 全面隔离、按设备串行化（阶段 3）；格式化与 lint 归一化（阶段 4）；Windows 服务就绪状态（阶段 5）；日志轮转与发布完整性（阶段 6）。

**硬约束**：37 个 v0.2.0 夹具必须继续匹配。任何响应漂移都是**契约变更**，必须显式提交并说明理由，不得靠放宽 `normalize` 掩盖。

</domain>

<decisions>
## Implementation Decisions

### 会话身份
- **D-01:** 采用**注册表 + 会话 ID**（方案 1A）。每连接分配不可猜测的会话 ID 并登记，断连时注销。理由是"设备拔出需失效相关会话"（SESS-04）在无注册表时无法实现——没有任何东西能定位到受影响的会话。
- **D-02:** 会话 ID 为 **32 位十六进制字符串**，由 `OsRng` 生成（`rand` 已是依赖，不引入 `uuid`）。
- **D-03:** 会话 ID **不下发给客户端**。一旦下发就会被当作凭据使用，从而与 SESS-04「断连即清除」矛盾。
- **D-04（明确排除）:** 不允许客户端重连后认领同一会话（方案 1C）。这与 SESS-04 直接冲突，不是暂缓项；若将来需要，必须先修改该需求。
- **D-04b:** 注册表**不得**变成"换了个名字的同一缺陷"：领域层代码只能访问自己会话的状态，不得按 ID 查找其他会话；跨会话访问仅允许出现在设备拔出失效这一条路径上。

### 授权状态
- **D-05:** 授权表示为 `(provider, device, app) -> expires_at`（方案 2A），满足 SESS-03。
- **D-06:** TTL 来自**环境变量 `SKF_AUTH_TTL_SECONDS`**，默认 **600 秒**。
- **D-07（具体陷阱，必须遵守）:** TTL **不得**写进 `config/skf.yaml`。`SkfConfig` 用 `#[serde(flatten)] libs: HashMap<String, HashMap<String,String>>` 捕获未知顶层键，插入 `session: {ttl_seconds: 600}` 会让 serde 尝试把 `600` 反序列化为 `String`，**整份配置解析失败**，直接违反 D-19。配置结构保持不变。
- **D-08:** `SKF_AUTH_TTL_SECONDS=0` **不表示"永不超时"**；无效值回落到默认值并在启动时记录警告。
- **D-09:** 清除触发为三条：断连、超时、以及**操作时检测到设备不可用**（方案 2-iii-a）。不做后台 `WaitForDevEvent` 广播——那会引入常驻阻塞线程并复杂化热路径。注册表为将来的实时失效预留了落点。
- **D-10:** PIN 验证完成后**不保留任何可恢复副本**（SESS-05）。授权条目只存 `expires_at`，不存 PIN 字符串。验证所需的 PIN 缓冲在处理函数内立即释放。

### 不透明句柄
- **D-11:** 句柄形态为**会话内计数器 + 类型前缀**（方案 3A）：`dev-1`、`app-1`、`cnt-1`、`hsh-1`。
- **D-12:** 未知、过期、类型错误的句柄统一返回**已有的 `-11`**（方案 3-i-a），按原因区分消息。
- **D-13（已冻结契约的边界）:** `DigestUpdate` 夹具断言的是 `{"error": -11, "message": "Invalid or expired hash handle"}`。若 digest 路径的消息文本发生变化，那是**契约变更**：必须在提交信息中显式声明，并同步更新夹具，**不得**通过放宽 `normalize` 掩盖。
- **D-14:** 句柄前缀**不出现在错误消息中**，避免把内部结构泄露给客户端。
- **D-15:** 句柄在会话结束时全部失效并释放；其他会话引用同一句柄必须被拒绝（RES-05）。

### 迁移范围
- **D-16:** 迁移**19 个安全相关分支**（方案 4A）至 provider：`CheckPIN`、`ConnectDev`、`DisConnectDev`、`GenerateRandom`、`CreateContainer`、`DeleteContainer`、`GetContainerType`、`ImportCertificate`、`SignData`、`RSASignData`、`CreatePKCS10`、`EncryptData`、`DecryptData`、`GenECCKeyPair`、`GenRSAKeyPair`、`DigestInit`、`DigestUpdate`、`DigestFinal`、`CloseHash`。
  其中 `ConnectDev`/`DisConnectDev`/`GenerateRandom` 必须在列——它们是 RES-01 的实际缺口入口。
- **D-17:** 其余 14 个分支（`SetLanguage`、`EnumProvider`、`EnumApplication`、`EnumContainer`、`FindCertificates`、`GetDevInfo`、`SetLabel`、`ECCVerify`、`RSAVerify`、`LockDev`、`UnlockDev`、`Transmit`、`Digest`、`IssueCertificate`）保持直连至阶段 3。它们不接收句柄、不验证 PIN，迁移收益低。
- **D-18:** 已路由的 4 个分支（`EnumDevice`、`GetDevState`、`WaitForDevEvent`、`CancelWaitForDevEvent`）保持在 provider 路径上，但需改为通过会话解析设备名，与 D-11 一致。

### 结构抽取（Phase 1 推迟项）
- **D-19:** 协议层抽取到 `protocol/`：`RpcRequest`、`RpcResponse`、`Language`、参数解析与校验。参数解析统一为类型化的提取辅助，取代逐分支的 `req.params.get(n)`。
- **D-20:** 领域层抽取到 `domain/`：D-16 列出的 19 个分支的处理函数。`main.rs` 保留组合根与 `Session`/`SessionFactory` 实现。
- **D-21:** 仍在 `main.rs` 的 14 个未迁移分支**不得**在本次搬迁中夹带行为变更；它们可以继续使用现有的直连路径。

### 兼容性与不变量（延续 Phase 1）
- **D-22:** 全 crate 仍只允许**一处** `unsafe impl Send/Sync`（在 `src/skf/types.rs`）；守卫继续以整数存储句柄。
- **D-23:** `SESS-02` 的验证方式必须是**对抗性测试**：两个并发会话，A 验证 PIN 后，B 的敏感操作仍须被拒绝。
- **D-24:** `SKF_CONFIG`、`SKF_WS_ADDR`、`SKF_HTTP_ADDR`、`RUST_LOG` 继续生效；两份 YAML 继续可解析且结构不变（D-19 的既有含义）。
- **D-25:** Windows i686 构建（`Machine=0x014C`）与 v0.2.0 JS 客户端契约继续成立。

### the agent's Discretion
- `protocol/`、`domain/`、`session/` 内部的文件划分与嵌套层级
- 类型化参数提取辅助的具体形态与命名
- 会话注册表的内部数据结构与清理时机细节
- 错误消息的具体措辞（digest 路径除外，见 D-13）
- 是否在诊断/日志中记录会话计数（不得记录会话 ID 之外的敏感内容）

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### 阶段范围与验收标准
- `.planning/ROADMAP.md` §Phase 2 — 目标、依赖、5 条成功标准
- `.planning/REQUIREMENTS.md` §Session and Authorization / §Resource Ownership — SESS-01..06、RES-01..05 是本阶段全部需求
- `.planning/PROJECT.md` — 里程碑目标、Constraints、Key Decisions

### Phase 1 产出（本阶段直接依赖）
- `.planning/phases/01-structural-foundation-and-test-seam/01-CONTEXT.md` — D-01..D-21，其中 D-07/D-08 的两项抽取被推迟到本阶段
- `.planning/phases/01-structural-foundation-and-test-seam/01-RESEARCH.md` §Pattern 2（由 37 个分支反推的操作组表）、§Pattern 3（fake 设计）— provider trait 的形状来源
- `.planning/phases/01-structural-foundation-and-test-seam/01-VERIFICATION.md` — Phase 1 已验证状态与 FOUND-04 的范围确认
- `.planning/phases/01-structural-foundation-and-test-seam/01-REVIEW.md` — M-02/M-03 与 6 条 low 的处置理由；M-01/M-04 已在 `01-REVIEW-FIX.md` 修复
- `tests/fixtures/v0.2.0/README.md` — 夹具的两条规则、跨进程确定性、时序敏感用例

### 研究与陷阱（决定本阶段做法）
- `.planning/research/ARCHITECTURE.md` §Target Architecture、§Session Layer、§Provider Layer — 会话与 provider 的职责边界
- `.planning/research/PITFALLS.md` — Pitfall 2（会话隔离从 provider 层泄漏）、Pitfall 3（重构与语义变更混合）、Pitfall 5（句柄跨任务共享）、Pitfall 6（"零化"假象）、Pitfall 12（过度顺从的 fake）
- `.planning/research/SUMMARY.md` §Phase 2 — 阶段理由与规避目标

### 现有源码
- `src/main.rs` — `SkfContext`（`pins`、`hash_handles`、`get_api`）、`handle_request` 的 37 个分支、`JsonRpcSession`/`JsonRpcSessionFactory`、`build_sessions`
- `src/provider/mod.rs` — `SkfProvider`、`DeviceGuard`（含 `begin_digest`）、`Operation`、`ProviderError`
- `src/provider/native.rs` — 守卫的 `Drop` 实现与单库实例
- `src/provider/fake.rs` — `fail_next` 与调用记录，SESS-02 的对抗性测试将依赖它
- `src/server/mod.rs` — `Session`/`SessionFactory` 接缝（`fn handle<'a>(&'a mut self, text) -> BoxFuture<'a, String>`）
- `src/config/mod.rs` — `SkfConfig` 的 `#[serde(flatten)]` 行为（D-07 的来源）
- `src/skf/types.rs` — 唯一的 `unsafe impl Send/Sync`
- `api/skf_api.js` — v0.2.0 客户端，参数顺序与响应形状的兼容基线

**本项目无外部规格或 ADR 文档** —— 需求与约束完整记录于上述 `.planning/` 文件与源码本身。

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **`Session`/`SessionFactory` 接缝**（Phase 1 建立）：`fn handle<'a>(&'a mut self, text: &'a str) -> BoxFuture<'a, String>`。会话状态的自然落点已经存在，本阶段只需在其上填充授权与句柄表。
- **provider 守卫**：`DeviceGuard`/`ApplicationGuard`/`ContainerGuard`/`DigestGuard` 已实现 `Drop` 释放，且 `fake` 记录 `Close*` 操作——RES-03/RES-04 的断言基础已就绪。
- **`FakeSkfProvider::fail_next`**：可注入 `DeviceRemoved`、`PinIncorrect` 等，SESS-02/D-23 的对抗性测试可直接使用。
- **`tests/fixtures/v0.2.0/`**：37 个夹具 + `tests/common/mod.rs` 的归一化，任何迁移都可立即验证是否漂移。
- **`Connection`/`get_api` 的 `Library::new` 现状**：11 处按次加载是 M-03 记录的遗留缺陷，本阶段迁移 19 个分支时一并消除。

### Established Patterns
- 错误处理：启动用 `anyhow` + context；请求边界用 `RpcResponse::err(code, msg, id)` 早返回。
- SKF 失败：`SAR_OK` 比较，十六进制码保留在消息中；`ProviderError::from_native` 已把 `SAR_COULDNOTGETFUNCADDR` 归一为 `SymbolUnavailable`。
- 门控：`#[cfg(windows)]` 在模块边界；`test-provider` feature 使 fake 不进入发布产物。
- 契约纪律：`normalize` 在录制时冻结；事后放宽等同契约变更。

### Integration Points
- `JsonRpcSessionFactory::create` 是分配会话 ID 与登记的唯一位置。
- `JsonRpcSession::handle` 是授权检查与句柄解析的入口；`Drop` 是会话注销与资源释放的落点（含异常断开）。
- `SkfContext` 目前是进程级单例，本阶段需要把 `pins`/`hash_handles` 从它迁出到会话，`config`/`provider` 保留为进程级。
- `build_sessions` 已接收 `&SkfConfig` 与 `&Arc<dyn SkfProvider>`，是注入 TTL 与注册表的自然位置。
- `DigestInit` 当前以 `hash_{id}` 为键存入进程级 `hash_handles`——本阶段改为会话级句柄表，这是 D-13 契约边界的直接来源。

</code_context>

<specifics>
## Specific Ideas

- 用户明确选择了**注册表**而非每连接对象，理由是"设备拔出需失效相关会话"必须有落点；同时要求注册表不得成为新的全局缺陷（D-04b 是这条约束的具体化）。
- 用户接受 **TTL 走环境变量**以避免破坏配置结构，而不愿为此放宽 `libs` 的类型。
- 明确了 **`-11` 复用**与**digest 消息文本属冻结契约**这一区分：错误码统一，但既有消息文本的改动要按契约变更走流程。
- 迁移范围锁定在**安全相关分支**，并特别把 `ConnectDev`/`DisConnectDev`/`GenerateRandom` 纳入——它们是已被证实能杀死进程的句柄入口。
- 会话 ID **不下发**给客户端，也不支持重连续用——后者与 SESS-04 冲突，被明确排除而非暂缓。

</specifics>

<deferred>
## Deferred Ideas

- **传输限制、连接数上限、帧大小检查**（`spawn_client` 当前无任何限制）→ 阶段 3（TRANS-01/02）
- **阻塞 FFI 全面移出异步 worker、按设备串行化** → 阶段 3（TRANS-04/05）。注意 `WaitForDevEvent` 必须继续留在阻塞池，这不是风格选择：Phase 1 已证实放上 worker 会让 cancel 一起阻塞。
- **其余 14 个只读分支的迁移** → 阶段 3（D-17）
- **`provider_load_failed` 消息中暴露库路径**（L-05）→ 阶段 6，OBS-04 脱敏工作时一并处理
- **`UnavailableProvider` 把消息放进名为 `path` 的字段**（L-01）→ 与 L-05 同批处理
- **固定 64 KiB 缓冲区与 `wait_for_event` 的入参长度**（L-03/M-02）→ 阶段 3 限制工作
- **`EXPECTED_METHODS` 手工维护**（L-06）→ 下次新增方法时一并处理
- **后台 `WaitForDevEvent` 实时失效广播** → 注册表已预留落点，待有实时性需求时再做（D-09）
- **`serde_yaml` 废弃依赖替换** → 独立里程碑

</deferred>

---

*Phase: 02-session-authorization-and-resource-ownership*
*Context gathered: 2026-09-11*

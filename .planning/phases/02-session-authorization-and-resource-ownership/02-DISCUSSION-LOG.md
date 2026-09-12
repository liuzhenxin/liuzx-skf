# Phase 2: Session Authorization and Resource Ownership - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-09-11
**Phase:** 02-session-authorization-and-resource-ownership
**Areas discussed:** 会话身份的载体, 授权状态的表示与失效, 不透明句柄的形态与拒绝语义, 未路由分支的迁移范围

---

## 会话身份的载体

| Option | Description | Selected |
|--------|-------------|----------|
| 注册表 + 会话 ID | 每连接分配不可猜测 ID 并登记，可枚举、可强制失效、可跨连接清理 | ✓ |
| 仅每连接对象，无注册表 | 改动最小，断连释放由 Drop 覆盖 | |
| 会话 ID 下发给客户端，允许重连续用 | 连续性强，但等于分发一个凭据 | |

**User's choice:** 注册表 + 会话 ID
**Notes:** 选择理由是"设备拔出需要失效相关会话"（SESS-04）必须有一个能定位受影响会话的落点——无注册表时该需求无法实现。讨论中明确点出第三项**与 SESS-04 直接冲突**（SESS-04 要求断连即清除授权），因此它不是"暂缓项"而是需要先改需求才可选的方案；用户接受这一判定。同时明确注册表不得退化为"换了名字的同一缺陷"：领域层只能访问自己会话的状态，跨会话访问仅允许出现在设备拔出失效这一条路径上。

### 子决策 1-i：会话 ID 形态
**User's choice:** 32 位十六进制，由 `OsRng` 生成
**Notes:** `rand` 已是依赖，不引入 `uuid`。

### 子决策 1-ii：ID 是否可见于客户端
**User's choice:** 不暴露
**Notes:** 一旦下发就会被当作凭据使用，与 SESS-04 的语义冲突。

---

## 授权状态的表示与失效

| Option | Description | Selected |
|--------|-------------|----------|
| `(provider, device, app) -> expires_at`，TTL 可配置 | 满足 SESS-03；断连、设备拔出、超时三条路径均可触发清除 | ✓ |
| 布尔值，仅断连清除 | 不满足 SESS-03 | |

**User's choice:** `(provider, device, app) -> expires_at`，TTL 可配置
**Notes:** 讨论中提出一条**由代码验证的具体陷阱**：`SkfConfig` 使用 `#[serde(flatten)] libs: HashMap<String, HashMap<String,String>>` 捕获未知顶层键，因此往 `config/skf.yaml` 加入 `session: {ttl_seconds: 600}` 会让 serde 尝试把 `600` 反序列化为 `String` 并使**整份配置解析失败**，直接违反 D-19。用户据此选择了环境变量方案，而不是为此放宽 `libs` 的类型。

### 子决策 2-i：TTL 来源
**User's choice:** 环境变量 `SKF_AUTH_TTL_SECONDS`
**Notes:** 零 YAML 风险，配置结构保持不变。

### 子决策 2-ii：默认 TTL
**User's choice:** 600 秒
**Notes:** 明确不接受 `0 = 永不超时`（那等价于被否决的布尔方案）。

### 子决策 2-iii：设备拔出的感知方式
**User's choice:** 操作时检测（下一次 open_container/verify_pin 返回 device-not-found 即失效该会话条目）
**Notes:** 放弃了后台 `WaitForDevEvent` 广播方案——实时性好但会引入常驻阻塞线程并复杂化热路径。注册表为将来需要实时性时预留了落点。

---

## 不透明句柄的形态与拒绝语义

| Option | Description | Selected |
|--------|-------------|----------|
| 会话内计数器 + 类型前缀 | 形如 `dev-1`、`cnt-3`、`hsh-2`；可读、可判定类型、天然限定在会话内 | ✓ |
| 全局 UUID | 不可预测性更强，但更长且无法从值判断类型 | |

**User's choice:** 会话内计数器 + 类型前缀
**Notes:** 讨论中特别指出 `DigestUpdate` 夹具已冻结 `{"error": -11, "message": "Invalid or expired hash handle"}`，因此句柄校验的**错误码**统一复用 `-11`，但 digest 路径的**消息文本**若变化属于契约变更，必须显式提交并同步更新夹具，不得靠放宽 `normalize` 掩盖。用户接受该区分。

### 子决策 3-i：拒绝的错误码
**User's choice:** 统一复用 `-11`，按原因区分消息（unknown / expired / wrong kind）
**Notes:** 不引入新稳定码；digest 路径保持既有消息文本，除非按契约变更流程处理。

### 子决策 3-ii：句柄前缀是否出现在错误消息中
**User's choice:** 不出现
**Notes:** 避免把内部结构泄露给客户端。

---

## 未路由分支的迁移范围

| Option | Description | Selected |
|--------|-------------|----------|
| 安全相关分支（约 19 个） | 涉及 PIN、句柄、证书、密钥的分支 | ✓ |
| 全部 33 个 | 最彻底，但把并发与限制工作一起卷进来 | |
| 仅 CheckPIN + 注册表 | 最小，但 RES-01 基本无法验证 | |

**User's choice:** 安全相关分支（19 个）
**Notes:** 讨论中强调 `ConnectDev`/`DisConnectDev`/`GenerateRandom` **必须**在列——它们是已证实能让服务进程崩溃的句柄入口，若排除则 RES-01 无法被验证。剩余 14 个为只读、不接收句柄、不验证 PIN 的分支，迁移收益低，留至阶段 3。

迁移清单（19）：`CheckPIN`、`ConnectDev`、`DisConnectDev`、`GenerateRandom`、`CreateContainer`、`DeleteContainer`、`GetContainerType`、`ImportCertificate`、`SignData`、`RSASignData`、`CreatePKCS10`、`EncryptData`、`DecryptData`、`GenECCKeyPair`、`GenRSAKeyPair`、`DigestInit`、`DigestUpdate`、`DigestFinal`、`CloseHash`

留至阶段 3（14）：`SetLanguage`、`EnumProvider`、`EnumApplication`、`EnumContainer`、`FindCertificates`、`GetDevInfo`、`SetLabel`、`ECCVerify`、`RSAVerify`、`LockDev`、`UnlockDev`、`Transmit`、`Digest`、`IssueCertificate`

---

## the agent's Discretion

- `protocol/`、`domain/`、`session/` 内部的文件划分与嵌套层级
- 类型化参数提取辅助的具体形态与命名
- 会话注册表的内部数据结构与清理时机细节
- 错误消息的具体措辞（digest 路径除外）
- 是否在诊断/日志中记录会话计数

## Deferred Ideas

- 传输限制、连接数上限、帧大小检查 → 阶段 3（TRANS-01/02）
- 阻塞 FFI 全面移出异步 worker、按设备串行化 → 阶段 3（TRANS-04/05）
- 其余 14 个只读分支的迁移 → 阶段 3
- `provider_load_failed` 消息中的库路径（L-05）、`UnavailableProvider` 的 `path` 字段（L-01）→ 阶段 6 脱敏工作
- 固定 64 KiB 缓冲区（L-03）与 `wait_for_event` 入参长度（M-02）→ 阶段 3 限制工作
- `EXPECTED_METHODS` 手工维护（L-06）→ 下次新增方法时
- 后台 `WaitForDevEvent` 实时失效广播 → 注册表已预留落点
- `serde_yaml` 替换 → 独立里程碑

## Constraints Carried Forward

讨论中确认延续，未产生新决策记录：

- 全 crate 仅一处 `unsafe impl Send/Sync`；守卫以整数存储句柄
- `SKF_CONFIG`、`SKF_WS_ADDR`、`SKF_HTTP_ADDR`、`RUST_LOG` 继续生效；两份 YAML 结构与语义不变
- Windows i686 构建（`Machine=0x014C`）与 v0.2.0 JS 客户端契约继续成立
- 37 个夹具必须继续匹配；任何漂移按契约变更流程处理
- `WaitForDevEvent` 必须继续留在阻塞池（Phase 1 已证实放上异步 worker 会让 cancel 一起阻塞）

---

*Phase: 02-session-authorization-and-resource-ownership*
*Discussion logged: 2026-09-11*

# Phase 3: Transport Hardening and Concurrency - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-09-11
**Phase:** 03-transport-hardening-and-concurrency
**Areas discussed:** 原生调用串行化 (TRANS-05), FFI 隔离与超时 (TRANS-04/TRANS-03), 边界数值 (TRANS-01/02/03), 非回环 opt-in (TRANS-06), 破坏性操作分类 (TRANS-07, 采用推荐默认)

---

## 原生调用串行化（TRANS-05）

### 2.1 粒度

| Option | Description | Selected |
|--------|-------------|----------|
| 每设备锁 | 同设备串行、不同设备并行；wait/cancel 无设备名仍需特殊处理 | |
| **每 provider 库全局锁 + 豁免 wait/cancel** | 实现简单、失败模式少；代价是不同设备也无法并行 | ✓ |
| 分层锁（全局 + 每设备） | 更细但更复杂，无证据支撑 | |

**User's choice:** `2.1b`
**Notes:** 厂商 DLL 线程安全性未知；当前以单把 GM3000 为主，并发代价可接受。硬约束：`WaitForDevEvent`/`CancelWaitForDevEvent` 必须豁免，否则 cancel 排在 wait 之后会死锁。

### 2.2 锁归属

| Option | Description | Selected |
|--------|-------------|----------|
| **native 内部共享锁** | `NativeSkfProvider` 持 `Arc<Mutex<()>>`，守卫共用；wait/cancel 不取锁 | ✓ |
| `SerializedProvider` 装饰器 | 覆盖不到守卫方法，需为四种守卫各写 wrapper | |
| dispatcher 层加锁 | 锁边界难判断，易漏 | |

**User's choice:** `2.2a`

### 2.3 只读是否并行

| Option | Description | Selected |
|--------|-------------|----------|
| **不区分，全部串行** | DLL 线程安全性未知；实现与测试面最小 | ✓ |
| 只读并行、危险串行 | 未经验证的吞吐换正确性 | |

**User's choice:** `2.3a`

### 2.4 锁实现

| Option | Description | Selected |
|--------|-------------|----------|
| **`std::sync::Mutex<()>` + 中毒恢复** | 阻塞上下文不跨 await；无新依赖 | ✓ |
| `parking_lot::Mutex` | 无中毒、更快但新增依赖 | |
| `tokio::sync::Mutex` | 鼓励在 async 上下文持锁，与 TRANS-04 冲突 | |

**User's choice:** `2.4a`
**Notes:** 锁必须与 `spawn_blocking` 同批落地，否则同库并发会卡住 worker 线程。

---

## FFI 隔离与超时（TRANS-04 / TRANS-03）

### 1.1 隔离机制

| Option | Description | Selected |
|--------|-------------|----------|
| **每调用点 `spawn_blocking`** | 延续 WaitForDevEvent 模式；守卫 move-in/move-out 辅助 | ✓ |
| 专用固定阻塞池 | 线程上界可控，但与全局锁高度重合、wait/cancel 更难 | |
| FFI actor | 语义清晰但改造与错误传播成本最大 | |

**User's choice:** `1.1a`

### 1.2 超时语义

| Option | Description | Selected |
|--------|-------------|----------|
| **30s 超时，仅止损** | 厂商线程继续执行到返回；wait/cancel 豁免 | ✓ |
| 不加超时 | 卡住的调用让同 provider 客户端无限等待 | |

**User's choice:** `1.2a`
**Notes:** 必须文档化"超时≠取消设备操作"。

### 1.3 超时错误形态

| Option | Description | Selected |
|--------|-------------|----------|
| **`-1` + 明确消息** | 夹具不覆盖挂起路径，不触发契约漂移 | ✓ |
| 新增专用错误码（如 `-12`） | 更可区分但扩展客户端可见错误面 | |

**User's choice:** `1.3a`

---

## 边界数值（TRANS-01 / TRANS-02 / TRANS-03）

### 3.1 帧/消息上限

| Option | Description | Selected |
|--------|-------------|----------|
| **1 MiB** | 256 KiB 载荷 base64 后约 350 KiB，留 2.5× 余量 | ✓ |
| 256 KiB | 更紧，大证书链可能触顶 | |
| 4 MiB | 宽松，单帧分配更大 | |

**User's choice:** `3.1a`

### 3.2 载荷上限（解码后）

| Option | Description | Selected |
|--------|-------------|----------|
| **256 KiB** | decode 前校验编码串长度 | ✓ |
| 1 MiB | | |
| 64 KiB | 可能影响批量 import/encrypt | |

**User's choice:** `3.2a`

### 3.3 并发连接数

| Option | Description | Selected |
|--------|-------------|----------|
| **64；超限立即拒绝并记录日志** | `try_acquire`，不入握手 | ✓ |
| 128；超限拒绝 | | |
| 64；超限排队 | 会让客户端挂起 | |

**User's choice:** `3.3a`

### 3.4 超时适用范围

| Option | Description | Selected |
|--------|-------------|----------|
| **仅 provider FFI** | wait/cancel 豁免；`IssueCertificate` OpenSSL 路径记为已知限制 | ✓ |
| 额外覆盖 `IssueCertificate` | 需为子进程加超时，复杂度上升 | |

**User's choice:** `3.4a`

---

## 非回环 opt-in（TRANS-06）

### 4.1 开关形式

| Option | Description | Selected |
|--------|-------------|----------|
| 仅环境变量 `SKF_ALLOW_REMOTE` | | |
| 仅 YAML 具名键 `allow_remote` | | |
| **两者，任一即 opt-in** | YAML 可审计；env 供运维/CI 覆盖 | ✓ |

**User's choice:** `4.1c`
**Notes:** 为 `SkfConfig` 增加具名字段不违反 D-07（D-07 是未命名嵌套键被 flatten 捕获）。

### 4.2 拒绝行为

| Option | Description | Selected |
|--------|-------------|----------|
| **启动即拒绝、非零退出、指出开关名** | 在 bind 之前校验 | ✓ |
| 回落到回环 + 警告 | 与 "refuses to bind" 不符 | |

**User's choice:** `4.2a`

### 4.3 覆盖范围

| Option | Description | Selected |
|--------|-------------|----------|
| **仅 WebSocket** | 保持 Console HTTP demo 的 `0.0.0.0:8000` 既有默认 | ✓ |
| WS + HTTP | 会破坏现有 demo 用法 | |

**User's choice:** `4.3a`

### 4.4 非回环判定

| Option | Description | Selected |
|--------|-------------|----------|
| **解析为 `SocketAddr` 后 `is_loopback`** | `0.0.0.0`/`::` 视为非回环 | ✓ |
| 仅按字面量匹配 | 漏掉 `127.0.0.2`、`::1`、`0.0.0.0` | |

**User's choice:** `4.4a`

---

## 破坏性操作分类（TRANS-07）

用户选择采用推荐默认，未逐项讨论：

- `pub enum OperationClass { ReadOnly, Destructive }` 作为代码真源，覆盖全部 37 个方法。
- `Destructive`：任何改变设备/令牌持久状态、消耗重试计数、生成/导入密钥、开关原生资源、改变锁状态的操作（含 `CheckPIN`、`CreateContainer`、`CreatePKCS10`、`Transmit` 等）。
- `ReadOnly`：纯查询与无状态计算（含 `SignData`、`EncryptData`、`GenerateRandom`）。
- 不变量测试断言每个 `EXPECTED_METHODS` 方法恰有一个分类。
- 本阶段只分类与文档，不据分类做强制。

---

## the agent's Discretion

- 锁辅助与 `spawn_blocking` 守卫搬迁辅助的形态与命名。
- `OperationClass` 模块位置与映射结构。
- 连接信号量、帧配置、载荷校验的接入点与消息措辞。
- 超时的具体包装方式（`tokio::time::timeout` 包裹 `JoinHandle`）。

## Deferred Ideas

- HTTP demo 的非回环默认（本阶段明确排除）。
- `IssueCertificate` 的 OpenSSL 子进程无界（已知限制）。
- 每设备细粒度并行（待 DLL 线程安全证据）。
- 自建阻塞池 / FFI actor（阶段 6 若需要）。
- 后台 `WaitForDevEvent` 实时失效广播（阶段 2 D-09 已推迟）。
- 据 `OperationClass` 强制授权/审计/确认（阶段 6）。
- `provider_load_failed` 暴露库路径（阶段 2 L-05，阶段 6）。
- 固定 64 KiB 缓冲区与 `wait_for_event` 入参长度（阶段 1 L-03/M-02）。

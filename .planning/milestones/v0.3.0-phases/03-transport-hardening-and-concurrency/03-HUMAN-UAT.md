---
status: partial
phase: 03-transport-hardening-and-concurrency
source: [03-VERIFICATION.md]
started: 2026-09-11
updated: 2026-09-11
---

## Current Test

[awaiting human testing — requires a GM3000 token and a Windows CI run]

## Tests

### 1. 真实设备上的并发调用串行化 (TRANS-05)
expected: 在两个连接上并发发起 `SignData`（或生成密钥），服务进程不崩溃，两次结果均正确；日志显示调用被串行化（无交叠）。
result: [pending]

### 2. 缓慢真实设备下无关客户端保持响应 (TRANS-03 / TRANS-04)
expected: 一个连接触发耗时设备操作时，另一连接执行 `SetLanguage` 或查询操作立即返回；async 运行时不被阻塞。
result: [pending]

### 3. 厂商 DLL 在锁之外的并发安全性 (TRANS-05)
expected: 记录该不确定性。若将来取得厂商线程安全文档，评估是否可放宽为每设备锁。
result: [pending]

### 4. Windows i686 产物 (D-25)
expected: 触发 `release-windows.yml`，产物 `skf-service.exe` 的 PE 头 `Machine=0x014C`。
result: [pending]

### 5. 超时后的连接状态 (REVIEW L-1)
expected: 超时（可用 `SKF_FFI_TIMEOUT_SECONDS=0` 复现）后，该连接的下一次请求以未授权/空句柄被拒，且原阻塞任务结束时资源被释放；重新连接后一切正常。
result: [pending]

## Summary

total: 5
passed: 0
issues: 0
pending: 5
skipped: 0
blocked: 0

## Gaps

---
status: partial
phase: 02-session-authorization-and-resource-ownership
source: [02-VERIFICATION.md]
started: 2026-09-11
updated: 2026-09-11
---

## Current Test

[awaiting human testing — requires a GM3000 token and a Windows CI run]

## Tests

### 1. 真实设备下的会话隔离 (SESS-02 增强)
expected: 会话 A `CheckPIN` 成功后，会话 B 的 `SignData` 必须以未授权错误（`-10`）被拒，而不是返回签名结果。
result: [pending]

### 2. 设备拔出后的失效行为 (SESS-04b)
expected: 会话 A 授权后拔出 Key；A 的下一次敏感操作以 device-not-found 被拒，且 A 的该设备授权条目被清除（再插回后仍需重新 `CheckPIN`）。
result: [pending]

### 3. Windows i686 产物 (D-25)
expected: 触发 `release-windows.yml`，产物 `skf-service.exe` 的 PE 头 `Machine=0x014C`。
result: [pending]

### 4. 真实设备上的释放 (RES-03/RES-04 补充)
expected: 在真实 token 上执行一次敏感操作并在中途断开连接；服务日志与设备状态表明 `CloseContainer`/`CloseApplication`/`CloseDevice` 均已释放（fake 注入已覆盖对应逻辑，此处确认真实调用）。
result: [pending]

## Summary

total: 4
passed: 0
issues: 0
pending: 4
skipped: 0
blocked: 0

## Gaps

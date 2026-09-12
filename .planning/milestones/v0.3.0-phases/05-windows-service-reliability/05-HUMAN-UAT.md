---
status: partial
phase: 05-windows-service-reliability
source: [05-VERIFICATION.md]
started: 2026-09-11
updated: 2026-09-11
---

## Current Test

[awaiting a Windows host — the SCM runtime, ACL enforcement, and file locks cannot
be exercised on the macOS development host]

Run on Windows as administrator:

```powershell
powershell -ExecutionPolicy Bypass -File packaging\windows\verify-service.ps1
```

That script covers tests 4, 5, 6 below automatically. Tests 1–3 need live SCM
observation and a deliberately invalid configuration.

## Tests

### 1. StartPending → Running 顺序 (SVC-01)
expected: 启动瞬间 `sc query LiuZXSKFService` 显示 `START_PENDING`（可通过 `verify-service.ps1 -KeepInstalled` 后在启动时观察）；监听器绑定完成后才变为 `RUNNING`。`status` 的 `stage` 依次为 config/provider/bind/serve。
result: [pending]

### 2. 无效配置触发非零退出与重启 (SVC-02)
expected: 把 `config\skf.yaml` 改坏（例如 `default` 指向不存在的别名）后启动服务；服务以 `ServiceSpecific` 非零码结束，`service-state.json` 为 `failed(stage=provider|config, code=1|2)`，且 `sc failure` 的重启动作按 5s/10s/60s 触发（事件日志可见）。
result: [pending]

### 3. `status` 区分"进程在跑"与"服务可用" (SVC-05)
expected: 占用 9001 端口使 bind 失败；`status` 显示 `Startup: Failed (stage=bind, code=3)` 与原因，而 SCM 可能显示服务已停止——两者可区分。
result: [pending]

### 4. 安装目录 ACL 阻止非管理员替换 (SVC-03)
expected: 以标准用户身份尝试覆盖 `%ProgramFiles(x86)%\LiuZX\SKF Service\skf-service.exe` 或 `mtoken_gm3000.dll` 必须被拒绝；`icacls` 中 `Users` 仅 `(OI)(CI)(RX)`。
result: [pending]

### 5. 运行中覆盖升级 (SVC-04)
expected: 服务运行中再次运行 `install.ps1`：服务停止、等待 exe 解锁、替换负载、重新注册并启动；升级后 `status` 仍为 `stage=serve`，无残留。
result: [pending]

### 6. 卸载无残留 + 重装 (SVC-04)
expected: `uninstall.ps1` 后 `Get-Service` 找不到服务且安装目录已删除；再次 `install.ps1` 成功。
result: [pending]

## Summary

total: 6
passed: 0
issues: 0
pending: 6
skipped: 0
blocked: 0

## Gaps

---
status: partial
phase: 06-observability-diagnostics-and-release-verification
source: [06-VERIFICATION.md]
started: 2026-09-12
updated: 2026-09-12
---

## Current Test

[awaiting external runs — a CI/release runner and a Windows host]

## Tests

### 1. 发布 workflow 的 checksum 与解包校验 (REL-03/REL-04)
expected: 触发 `release-windows.yml`（tag 或手动）；"Verify checksum" 与 "Verify extracted package contents and architecture" 通过；`Machine=0x014C` 对 exe 与 DLL 成立；ZIP 与 `.sha256` 两个附件都上传/发布。
result: [pending]

### 2. 长期运行下的日志轮转与保留 (OBS-01)
expected: 在 Windows VM 上让服务运行到超过 10 MiB 日志，`logs\` 下出现 `skf-service.log.1..7`，总数不超过 8；每行可被 JSON 解析。
result: [pending]

### 3. 真实机器上的 diagnose (OBS-03/OBS-04)
expected: `skf-service.exe diagnose --json` 输出五阶段布尔、provider 别名、端口、错误类别；断连厂商库时 `library_file`/`library_loaded` 为 false 且不含任何路径。
result: [pending]

### 4. Windows 服务生命周期（承接阶段 5）
expected: `packaging\windows\verify-service.ps1` 全绿（install → status(stage=serve) → 覆盖升级 → ACL → uninstall → reinstall），并确认 `service-state.json` 含 `started_at`。
result: [pending]

## Summary

total: 4
passed: 0
issues: 0
pending: 4
skipped: 0
blocked: 0

## Gaps

---
status: partial
phase: 04-format-lint-normalization-and-ci-gate
source: [04-VERIFICATION.md]
started: 2026-09-11
updated: 2026-09-11
---

## Current Test

[awaiting remote/organization actions — repository files cannot configure these]

## Tests

### 1. 分支保护阻断合并 (QUAL-04)
expected: GitHub Settings → Branches 对 `main` 启用 "Require status checks to pass"，并勾选 `fmt`、`clippy`、`test`、`test-full`、`windows-i686`、`gate-selftest`；此后红灯 PR 无法合并。清单见 `docs/CI.md`。
result: [pending]

### 2. x86_64 macOS runner 标签可用 (QUAL-03)
expected: 首次 workflow 运行时 `test-full` 在 `macos-13`（或替代的 x86_64 macOS 标签）上成功启动并加载已提交的 x86_64 厂商 dylib。
result: [pending]

### 3. 首次真实 PR 上 6 个 job 全部出现 (QUAL-03)
expected: 开一个 draft PR，确认 6 个 job 全部运行；`test-full` 的契约重放为 37/37；`gate-selftest` 输出 "gate is failure-sensitive"。
result: [pending]

## Summary

total: 3
passed: 0
issues: 0
pending: 3
skipped: 0
blocked: 0

## Gaps

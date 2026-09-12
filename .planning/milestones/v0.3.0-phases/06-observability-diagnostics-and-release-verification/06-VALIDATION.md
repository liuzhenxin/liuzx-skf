---
phase: 6
slug: observability-diagnostics-and-release-verification
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-09-11
---

# Phase 6 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `cargo test`；PowerShell for the release verification |
| **Config file** | none（无新依赖；见 RESEARCH §0） |
| **Quick run command** | `cargo test --lib` |
| **Full suite command** | `cargo test` + `cargo fmt --all -- --check` + `cargo clippy --all-targets -- -D warnings` + `cargo check --target i686-pc-windows-gnu` |
| **Estimated runtime** | ~25 s |

---

## Sampling Rate

- **After every task commit:** `cargo test --lib`
- **After every plan wave:** the full suite above
- **Before `$gsd-verify-work`:** full suite green；`contract_fixtures` green
- **Max feedback latency:** ~25 s

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 06-01-01 | 06-01 | 1 | OBS-01 | — | JSON-line format + rotation + retention | unit | `cargo test logging` | ❌ W1 | ⬜ pending |
| 06-01-02 | 06-01 | 1 | OBS-01 | — | service initialization uses the rotating logger | structural | `grep -c 'logging::init_service_file' src/win_service.rs` == 1 | ✅ | ⬜ pending |
| 06-01-03 | 06-01 | 1 | OBS-02 | T-06-01 | `sanitize` + no sensitive values in logs | unit + scan | `cargo test --test log_redaction` | ❌ W1 | ⬜ pending |
| 06-02-01 | 06-02 | 2 | OBS-03 | — | diagnose reports five stage booleans | unit | `cargo test diagnostic` | ❌ W2 | ⬜ pending |
| 06-02-02 | 06-02 | 2 | OBS-04 | T-06-02 | diagnose output has no path/secret | unit | `cargo test diagnostic` | ❌ W2 | ⬜ pending |
| 06-02-03 | 06-02 | 2 | REL-01 | T-06-03 | apiVersion absent/present/too-new | integration | `cargo test --test protocol_version` | ❌ W2 | ⬜ pending |
| 06-02-04 | 06-02 | 2 | REL-02 | T-06-04 | restricted methods present; gated only when opted in | unit | `cargo test restricted` | ❌ W2 | ⬜ pending |
| 06-03-01 | 06-03 | 3 | REL-03 | — | checksum file generated | structural | `grep -c 'sha256' packaging/windows/build.ps1` >= 1 | ✅ | ⬜ pending |
| 06-03-02 | 06-03 | 3 | REL-04 | — | workflow verifies extracted contents + 0x014C | structural | `grep -c '0x014C\|Get-PeMachine' release-windows.yml` >= 1 | ✅ | ⬜ pending |
| 06-03-03 | 06-03 | 3 | REL-05 | — | release notes list behavior changes + migration | docs | `test -f RELEASE-NOTES.md` | ❌ W3 | ⬜ pending |
| 06-03-04 | 06-03 | 3 | DOC-01 | — | threat model documents the boundary | docs | `test -f THREAT-MODEL.md` | ❌ W3 | ⬜ pending |
| 06-03-05 | 06-03 | 3 | DOC-02 | — | bilingual session/limits doc | docs | `grep -c 'English\|中文' docs/SESSION-AND-LIMITS.md` >= 2 | ❌ W3 | ⬜ pending |
| 06-03-06 | 06-03 | 3 | DOC-03 | — | agent/README docs reflect the new layout | docs | `grep -c 'diagnose' AGENTS.md` >= 1 | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] Rust toolchain + i686 target + existing deps（阶段 4）
- [x] `service_state.rs`（阶段 5，本阶段新增 `started_at`）
- [ ] `src/logging.rs`（06-01-01）
- [ ] `src/diagnostic.rs`（06-02-01）
- [ ] `RELEASE-NOTES.md` / `THREAT-MODEL.md` / `docs/SESSION-AND-LIMITS.md`（06-03）

**无新依赖**：见 RESEARCH §0（registry 不可达，`flexi_logger` 不可加入）。

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| 轮转在真实 Windows 服务下产生 `.1..7` | OBS-01 | 需要长时间运行的真实服务 | 在 Windows VM 持续运行并检查 `logs\skf-service.log*` 数量与 JSON 行 |
| 发布 workflow 的 checksum/解包校验 | REL-03/04 | 需要 CI/tag | 触发 `release-windows.yml`，确认校验步骤通过且两个附件上传 |
| diagnose 在真实机器上的五阶段输出 | OBS-03 | 需要厂商库/端口占用等环境 | 在 Windows VM 运行 `skf-service.exe diagnose --json` |

---

## Validation Sign-Off

- [ ] 每个任务具备自动化命令或人工指令
- [ ] 采样连续性：不存在连续 3 个任务缺少自动化验证
- [ ] Wave 0 覆盖全部缺失引用
- [ ] 无 watch-mode 标志
- [ ] 反馈延迟 < 25 s
- [ ] 计划完成后置 `nyquist_compliant: true`

**Approval:** pending

---

*Phase: 06-observability-diagnostics-and-release-verification*
*Validation strategy created: 2026-09-11*

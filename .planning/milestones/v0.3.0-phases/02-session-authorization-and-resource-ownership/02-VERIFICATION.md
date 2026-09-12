---
phase: 02
slug: session-authorization-and-resource-ownership
status: passed
verified_at: 2026-09-11
must_haves_total: 6
must_haves_verified: 6
must_haves_partial: 0
requirements_verified: [SESS-01, SESS-02, SESS-03, SESS-04, SESS-05, SESS-06, RES-01, RES-02, RES-03, RES-04, RES-05]
requirements_partial: []
human_verification_count: 4
verifier: inline (no subagent runtime available)
---

# Phase 2 — Verification

**Phase goal:** 消除共享凭据与客户端可控原生句柄两个最严重缺陷：授权按会话隔离并自动失效，所有原生资源有明确归属和确定性释放。

**Verdict:** `passed`. Every automated check passes, RES-04 is proven by
fake-injected failures on every migrated error path, the frozen v0.2.0 contract
is 37/37, and the fifteen unmigrated branches are byte-identical to the
pre-plan tree. Four items require a real token or a Windows CI runner and are
recorded for human verification.

## Automated Evidence

| Check | Command | Result |
|-------|---------|--------|
| Full suite | `cargo test` | 109 passed, 0 failed, 1 ignored |
| Lib tests | `cargo test --lib` | 85 passed |
| Contract replay | `cargo test --test contract_fixtures` | 37 compared, 0 failures |
| Oracle self-check | `cargo test --test fixture_oracle` | 4 passed |
| Provider invariants | `cargo test --test provider_invariants` | 6 passed |
| Session invariants | `cargo test --test session_invariants` | 6 passed |
| Session isolation | `cargo test --test session_isolation` | 2 passed |
| Session lifecycle | `cargo test --test session_lifecycle` | 2 passed |
| Warnings | `cargo check --all-targets` | 0 |
| Windows-side compile | `cargo check --target i686-pc-windows-gnu` | success |
| Schema drift | `gsd-tools verify schema-drift 02` | `drift_detected: false` |

## must_haves

| # | Must-have | Status | Evidence |
|---|-----------|--------|----------|
| 1 | `protocol/` provides typed parameter extraction with pre-refactor error codes/messages | **Met** | `src/protocol/params.rs`; 12 protocol tests; contract fixtures unchanged |
| 2 | 12 branches migrated to `domain/`; `main.rs` keeps only the composition root, `Session` impl, and the 14 D-17 branches | **Met** | 12 `pub async fn handle`; `main.rs` 3302 → 1989 lines; 15 unmigrated branches byte-identical to HEAD |
| 3 | Every domain native error path releases resources (guard RAII) | **Met** | 12 `*_releases_on_native_error` tests assert each `Close*` exactly once |
| 4 | `cargo test --test contract_fixtures` 37/37 | **Met** | 37 compared, 0 failed |
| 5 | `cargo check --target i686-pc-windows-gnu` passes | **Met** | success, 0 warnings |
| 6 | Exactly one `unsafe impl Send` and one `unsafe impl Sync` remain | **Met** | `tests/provider_invariants.rs::only_one_unsafe_send_sync_impl_exists` |

## Requirements

| Requirement | Status | Evidence |
|-------------|--------|----------|
| SESS-01 session identity (OsRng, 32 hex, not sent) | Met | `session::tests::session_ids_are_32_lowercase_hex_characters`, `session_ids_do_not_repeat` |
| SESS-02 authorization is per session | Met (automated), enhanced by human item | `two_sessions_do_not_share_authorization`; wire-level `session_isolation` asserts no handle leaks across connections |
| SESS-03 authorization expires after TTL | Met | `session::auth::tests::expiry_removes_the_grant`, `ttl_rejects_zero_and_garbage` |
| SESS-04 cleared on disconnect / device removal | Met (disconnect automated), device-removal by human item | `registry_does_not_keep_sessions_alive`, `session_clears_grant_when_device_reported_removed`; `session_lifecycle` smoke test |
| SESS-05 no recoverable PIN retained | Met | `a_grant_holds_only_a_deadline`; `ctx.pins` count 0 |
| SESS-06 stable rejection for unauthorized sessions | Met | `domain::pin` tests; handlers return `-10` |
| RES-01 opaque handles replace client-controlled integers | Met | `session_isolation` (fabricated integer → `-11`); `as DEVHANDLE` count 0 |
| RES-02 unknown/expired/wrong-kind rejected without leaking format | Met | `session::handles` tests; fieldless `HandleError` |
| RES-03 release on disconnect (including abrupt) | Met | `dropping_the_table_releases_every_resource`; `session_lifecycle` |
| RES-04 release on native error paths | Met | 12 `*_releases_on_native_error` tests |
| RES-05 cross-session handle unusability | Met | table is session-owned; `session_isolation` guessed-handle test |

## Human Verification Required

`02-HUMAN-UAT.md` records these; they are not automated because the machine has
no GM3000 token and is not a Windows runner.

1. 真实设备下的会话隔离 — A `CheckPIN` 成功后 B `SignData` 被拒。
2. 设备拔出后的失效 — A 授权后拔出 Key，下一次敏感操作以 device-not-found 被拒且授权清除。
3. Windows i686 产物 — `release-windows.yml` 触发并以 `Machine=0x014C` 为证据。
4. 真实设备上的释放 — 拔出/断连后 `Close*` 实际调用。

## Notes

- The verifier ran inline because this runtime exposes no `Task` subagent API;
  the same commands the `gsd-verifier` agent would run were executed directly,
  and every claim above maps to a file or command output.
- One plan acceptance criterion (`grep -c 'Library::new' src/main.rs` ≤ 1) is
  unsatisfiable within the plan's own scope; the four remaining direct-load sites
  belong to D-17/Phase 3 and are documented in `02-04-SUMMARY.md`.

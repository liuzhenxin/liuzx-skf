---
gsd_state_version: 1.0
milestone: v0.4.0
milestone_name: Secure Remote Operation
status: Ready to execute
stopped_at: Phase 8 executed and verified
last_updated: "2026-09-13T13:29:47.461Z"
progress:
  total_phases: 4
  completed_phases: 2
  total_plans: 8
  completed_plans: 5
  percent: 50
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-09-12)

**Core value:** 应用能够通过稳定、安全且与厂商实现解耦的统一接口访问 USB Key 的硬件密码能力。
**Current Milestone:** v0.4.0 Secure Remote Operation — TLS, client authentication, tightened bind policy, authorization audit.

## Current Position

Milestone: v0.4.0 Secure Remote Operation — IN PROGRESS
Phase: 8 (Client Authentication) — COMPLETE
Next action: `$gsd-plan-phase 9` (Bind Policy and Authorization Audit)
Last Activity Description: Phase 9 planning complete — 3 plans ready

Progress: [██████░░░░] 50%  (2/4 phases)

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table. Recent decisions affecting current work:

- Milestone v0.4.0 is the security-exposure milestone: TLS + client authentication + tightened bind policy + authorization audit.
- v0.3.0/v0.3.1 shipped and verified: session-scoped authorization, opaque handles, bounded transport, FFI serialization, CI gate + branch protection, honest Windows service, structured logs/diagnostics, checksummed releases.
- [v0.3.1] A `ConnectDev` failure (token removed) must clear the device's grants — found by real-hardware UAT B2, fixed in v0.3.1.
- [v0.3.0] The 37 v0.2.0 fixtures stay frozen; `WaitForDevEvent` and the OpenSSL-dependent `IssueCertificate` mock are shape-checked, `normalize` untouched.
- [Phase 2] Per-operation PIN re-verification removed by design; the TTL-bounded grant is the authorization, and no PIN copy is retained.
- [Phase 3] Blocking FFI runs on the blocking pool behind one per-provider lock; `WaitForDevEvent`/`CancelWaitForDevEvent` are exempt.
- [Phase 4] rustfmt/clippy clean, toolchain pinned 1.93.0, CI requires the five protected checks.

### Pending Todos

None.

### Blockers/Concerns

- TLS/client-auth crate availability: the local cargo registry needed a sparse mirror; `~/.cargo/config.toml` was switched to `sparse+https://mirrors.tuna.tsinghua.edu.cn/crates.io-index/` (backup at `~/.cargo/config.toml.bak`). CI fetches from crates.io normally.
- Vendor DLL thread safety remains unproven; per-device concurrency stays deferred.
- The HTTP demo can still bind non-loopback in console mode (documented boundary).
- [Phase 7/8] The 37-fixture contract replay cannot run on the Mac while the GM3000 token is attached to the Windows VM (`ConnectDev` returns 0x00000001 instead of the recorded 0x0A000023). Re-run with the token on the Mac before the v0.4.0 audit.
- [Phase 8] `client_auth: none` is now loopback-only; phase 9 will additionally require TLS for any non-loopback bind.
- `IssueCertificate` remains a Mock (real CA integration deferred).

## Deferred Items

Carried into the v0.4.0 backlog (not in this milestone): per-device concurrency/throughput, multi-vendor (FishMan/3000GM) and Linux/macOS distribution, remote log shipping/metrics/health endpoint, real CA integration.

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| concurrency | per-device locking / multi-device throughput | deferred | v0.4.0 start |
| distribution | FishMan / 3000GM / Linux / macOS packaging | deferred | v0.4.0 start |
| observability | remote log shipping, metrics, health endpoint | deferred | v0.4.0 start |
| ca | real certificate issuance (IssueCertificate) | deferred | v0.4.0 start |

## Session Continuity

Last session: 2026-09-13T11:40:00.000Z
Stopped at: Phase 8 executed and verified
Resume file: .planning/phases/08-client-authentication/08-VERIFICATION.md

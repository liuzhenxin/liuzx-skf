---
gsd_state_version: 1.0
milestone: v0.4.0
milestone_name: Secure Remote Operation
status: Milestone v0.4.0 shipped — awaiting next milestone
stopped_at: Milestone v0.4.0 completed, archived, and tagged
last_updated: "2026-09-13T14:30:00.000Z"
last_activity: 2026-09-13 — Milestone v0.4.0 completed and archived
progress:
  total_phases: 4
  completed_phases: 4
  total_plans: 11
  completed_plans: 11
  percent: 100
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-09-12)

**Core value:** 应用能够通过稳定、安全且与厂商实现解耦的统一接口访问 USB Key 的硬件密码能力。
**Shipped:** v0.3.0 / v0.3.1 (2026-09-12) and v0.4.0 (2026-09-13). No active milestone — start the next with `$gsd-new-milestone`.

## Current Position

Phase: Milestone v0.4.0 complete
Plan: —
Status: Awaiting next milestone
Last activity: 2026-09-13 — Milestone v0.4.0 completed and archived

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
- [v0.4.0] A non-loopback bind requires opt-in + TLS + client authentication; `client_auth: none` is accepted only on loopback.
- [v0.4.0] The contract replay (37 fixtures) passes on this host with the GM3000 token attached; it is a local/pre-release check, not a hosted CI job.
- `IssueCertificate` remains a Mock (real CA integration deferred).

## Deferred Items

Open artifact audit at v0.4.0 close: all clear. The items below are the
acknowledged backlog carried forward, not milestone gaps.

Carried into the next milestone: per-device concurrency/throughput, multi-vendor
(FishMan/3000GM) and Linux/macOS distribution, remote log shipping/metrics/health
endpoint, real CA integration.

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| concurrency | per-device locking / multi-device throughput | deferred | v0.4.0 close |
| distribution | FishMan / 3000GM / Linux / macOS packaging | deferred | v0.4.0 close |
| observability | remote log shipping, metrics, health endpoint | deferred | v0.4.0 close |
| ca | real certificate issuance (IssueCertificate) | deferred | v0.4.0 close |
| verification | real operator CA mTLS + Windows service TLS lifecycle UAT | deferred | v0.4.0 close |

## Session Continuity

Last session: 2026-09-13T14:30:00.000Z
Stopped at: Milestone v0.4.0 completed and archived
Resume file: .planning/milestones/v0.4.0-MILESTONE-AUDIT.md

## Operator Next Steps

- Start the next milestone with /gsd-new-milestone

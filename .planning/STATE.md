---
gsd_state_version: 1.0
milestone: v0.4.0
milestone_name: Secure Remote Operation
status: Phase 9 complete
stopped_at: Phase 9 executed and verified; contract fixtures now green
last_updated: "2026-09-13T13:35:00.000Z"
progress:
  total_phases: 4
  completed_phases: 3
  total_plans: 8
  completed_plans: 8
  percent: 75
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-09-12)

**Core value:** 应用能够通过稳定、安全且与厂商实现解耦的统一接口访问 USB Key 的硬件密码能力。
**Current Milestone:** v0.4.0 Secure Remote Operation — TLS, client authentication, tightened bind policy, authorization audit.

## Current Position

Milestone: v0.4.0 Secure Remote Operation — IN PROGRESS
Phase: 9 (Bind Policy and Authorization Audit) — COMPLETE
Next action: `$gsd-plan-phase 10` (Documentation and Quality Closeout)
Last Activity Description: Phase 9 executed — combined exposure rule + structured audit; contract fixtures green

Progress: [████████░░] 75%  (3/4 phases)

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
- [RESOLVED in Phase 9] The 37-fixture contract replay was blocked while the GM3000 token was attached to the Windows VM; the token is back on this Mac and `contract_fixtures` is now 4/4 green.
- [Phase 9] A non-loopback bind now requires opt-in + TLS + client authentication; `client_auth: none` is accepted only on loopback.
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

Last session: 2026-09-13T13:35:00.000Z
Stopped at: Phase 9 executed and verified
Resume file: .planning/phases/09-bind-policy-and-authorization-audit/09-VERIFICATION.md

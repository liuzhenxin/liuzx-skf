---
phase: 02-session-authorization-and-resource-ownership
plan: 02
subsystem: session
tags: [authorization, ttl, pin-hygiene, credential-isolation]

requires:
  - phase: 02-01
    provides: SessionState, SessionGuard, and the registry
provides:
  - src/session/auth.rs with AuthTable, AuthKey, Grant, and ttl_from_env
  - Per-session authorization wired through the dispatcher; CheckPIN grants instead of caching
  - Device-loss paths that clear the affected device's grants
  - SkfContext with no PIN map at all
affects: [02-03, 02-04, phase-6 observability work]

tech-stack:
  added: []
  patterns:
    - "Authorization is a deadline per (provider, device, application); the credential is not stored"
    - "TTL from the environment, never from YAML, because the config uses a flattened provider map"
    - "Device loss detected by the session that observes it, using its own &mut"

key-files:
  created:
    - src/session/auth.rs
  modified:
    - src/session/mod.rs
    - src/main.rs
    - src/lib.rs

key-decisions:
  - "Per-operation PIN re-verification is removed by design: the session grant is the authorization"
  - "SKF_AUTH_TTL_SECONDS=0 means 'invalid', not 'never expires', because SESS-03 requires expiry"
  - "Each migrated rejection site keeps its original -10 message verbatim, including the one variant"

patterns-established:
  - "Never store a credential to satisfy a 'do not retain' requirement; store the fact of verification instead"
  - "Configuration keys that would break a flattened serde map go to the environment instead"

requirements-completed: [SESS-02, SESS-03, SESS-05, SESS-06]

duration: 75min
completed: 2026-09-11
---

# Phase 2 Plan 2: Per-Session Authorization with Expiry Summary

**Authorization now belongs to one connection: a deadline per application, no PIN retained anywhere, and a process-wide cache that no longer exists.**

## Performance

- **Duration:** 75 min
- **Started:** 2026-09-11T15:40:00Z
- **Completed:** 2026-09-11T16:55:00Z
- **Tasks:** 3 completed
- **Files modified:** 1 created, 3 modified

## Accomplishments

- **SESS-02 satisfied, adversarially.** `two_sessions_do_not_share_authorization` grants in one session and asserts the other is rejected for the same `(provider, device, application)`. Before this change the grant lived in a process-wide map and that test could not have been written.
- **SESS-03 satisfied.** `expiry_removes_the_grant` uses a 50 ms TTL and asserts the check returns `Expired` **and** that the entry is gone — expiry removes rather than reports, so nothing inspecting the table can still see a stale grant.
- **SESS-05 satisfied structurally.** `Grant` holds only an `Instant`; a test pins its size and `Copy`-ness so a credential field cannot creep in. `grep -c 'pins' src/main.rs` → 0.
- **SESS-06 satisfied.** Unauthorized operations keep the pre-refactor `-10` code and message, so the rejection contract is unchanged.
- **Every one of the 12 former PIN-cache sites now confirms a live session grant**, and each retains its original rejection message verbatim — including the single variant at the old line 1339 (`"User not logged in (PIN cache empty). Call CheckPIN first."`).
- **69 tests pass, 0 warnings, 37/37 fixtures still match.** The fixtures matter here: `CheckPIN` and `DeleteContainer` are covered, and their recorded rejections (`ConnectDev failed` / `-10`) are byte-identical after the change.

## Task Commits

1. **Task 1: 实现会话内授权表与 TTL** — `43506b0` (feat)
2. **Task 2: 把 CheckPIN 与 PIN 缓存迁到会话** — `43506b0` (feat)
3. **Task 3: 接入断连与设备不可用时的授权清除** — `43506b0` (feat)

## Decisions Made

- **Per-operation PIN re-verification is gone, deliberately.** The old code re-verified the PIN with the token on every sensitive operation. It cannot do that any more, because the PIN is not retained. The TTL-bounded session grant replaces it. This is the behaviour change the milestone authorized; it is recorded here rather than buried, and no fixture covers the authorized path (there is no token), so the recorded contract is unaffected.
- **`SKF_AUTH_TTL_SECONDS=0` is invalid, not infinite.** SESS-03 requires expiry, so an unbounded grant must not be expressible. Zero, non-numeric, and negative inputs fall back to 600 s with a warning.
- **The TTL is not a configuration file key.** `SkfConfig` captures unknown top-level keys through a flattened `HashMap<String, HashMap<String, String>>`, so adding `session: {ttl_seconds: 600}` would make serde try to deserialize `600` as a `String` and fail the entire file. The environment is the safe channel.
- **Device-loss grants are cleared where the loss is observed.** Every `OpenApplication` failure path calls `device_unavailable`, since a missing application is how a removed device surfaces there.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] The regex-based migration dropped a `let extra` binding**
- **Found during:** Task 3 (`cargo check`)
- **Issue:** inserting the `device_unavailable` call consumed the `let extra = ...` line at the `CheckPIN` branch, producing `cannot find value extra`.
- **Fix:** restored the binding before the return so the message format `"OpenApplication failed: 0x{:08X}{}"` keeps its `(Application Not Exists)` suffix.
- **Verification:** `cargo check --all-targets` clean; `cargo test --test contract_fixtures` green.
- **Committed in:** `43506b0`

**2. [Rule 1 - Bug] Three `pin_key` bindings became dead code**
- **Found during:** Task 3 (`cargo check --all-targets` warnings)
- **Issue:** after replacing the cache lookups, three `let pin_key = format!(...)` bindings were unused. Silencing them with an underscore would have left a misleading artefact.
- **Fix:** removed them; `grep -c 'let pin_key'` → 0.
- **Verification:** 0 warnings from `cargo check --all-targets`.
- **Committed in:** `43506b0`

---

**Total deviations:** 2 auto-fixed (2 × Rule 1)
**Impact on plan:** Both were mechanical consequences of the migration. No scope change.

## Issues Encountered

- **The migration touched 12 sites with four different local shapes** (two comment wordings, two `pin_str`/`pin_str.as_str()` forms, one distinct rejection message). Each was transformed programmatically and then verified by the fixtures rather than by reading alone — which is what caught deviation 1.

## User Setup Required

None. Operators may set `SKF_AUTH_TTL_SECONDS` to change the authorization lifetime.

## Next Phase Readiness

Plan 02-03 adds the session-owned handle table, which is where RES-01/02/03/05 and the two verified defects (V-1 abandoned digest handles, V-2 per-call library loads) are resolved.

**Carried forward:**
- The launchd agent stays unloaded; the Windows VM is untouched.
- `SessionState` still has no handle table, so nothing yet releases native resources on disconnect — that is 02-03's job.

## Self-Check: PASSED

- `cargo test session` → 20 passed
- `cargo test` → 69 passed, 0 failed
- `cargo check --all-targets` → 0 warnings
- `grep -c 'ctx\.pins' src/main.rs` → 0; `grep -c 'pins' src/main.rs` → 0
- `grep -c 'session_authorized(' src/main.rs` → 12
- `cargo test --test contract_fixtures` → 37 compared, 0 failures

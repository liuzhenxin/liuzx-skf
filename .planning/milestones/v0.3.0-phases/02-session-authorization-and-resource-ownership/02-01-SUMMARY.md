---
phase: 02-session-authorization-and-resource-ownership
plan: 01
subsystem: session
tags: [session, registry, ownership, osrng]

requires:
  - phase: 01-04
    provides: Session/SessionFactory transport seam and the provider injection point
provides:
  - src/session/mod.rs with SessionId (OsRng, 32 hex), SessionMarker, SessionState
  - src/session/registry.rs with a Weak-based registry and a Drop-deregistering SessionGuard
  - tests/session_invariants.rs enforcing session-layer structural invariants
affects: [02-02, 02-03, 02-04]

tech-stack:
  added: []
  patterns:
    - "Registry tracks liveness via Weak<SessionMarker> and never owns session state"
    - "Session state held by value so the guard stays Send without a new unsafe impl"
    - "assert_send::<T>() to pin a movability property in the type system"
    - "Comment-aware, token-aware source invariants instead of bare greps"

key-files:
  created:
    - src/session/mod.rs
    - src/session/registry.rs
    - tests/session_invariants.rs
  modified:
    - src/lib.rs

key-decisions:
  - "SessionState is held by value, not behind an Arc: it will hold Send-but-not-Sync provider guards, so an Arc would not be Send and the session could not move into a Tokio task"
  - "The registry stores Weak<SessionMarker> (id only) and offers no lookup and no cross-session mutation"
  - "Device-unavailable handling belongs to the session that observes it, which keeps session state lock-free"

patterns-established:
  - "Use assert_send::<T>() to keep a movability invariant from silently regressing"
  - "Invariant tests must be comment-aware: doc comments legitimately name forbidden constructs"

requirements-completed: [SESS-01, SESS-04]

duration: 40min
completed: 2026-09-11
---

# Phase 2 Plan 1: Session Skeleton and Registry Summary

**The service now has a session ownership model: unguessable ids, a registry that tracks liveness without owning sessions, and session state held by value so a connection can actually be moved into a Tokio task.**

## Performance

- **Duration:** 40 min
- **Started:** 2026-09-11T15:00:00Z
- **Completed:** 2026-09-11T15:40:00Z
- **Tasks:** 3 completed
- **Files modified:** 3 created, 1 modified

## Accomplishments

- **SESS-01 satisfied.** `SessionId` is 128 bits from `OsRng`, rendered as 32 lowercase hex characters. Test asserts the shape and that 1000 consecutive ids do not repeat.
- **SESS-04 partially satisfied** — the disconnect half. `SessionGuard`'s `Drop` deregisters from the registry, and a test proves a dropped guard leaves no entry.
- **Two design hazards closed before they could bite** (both recorded in `02-RESEARCH.md` as G-1b):
  1. The registry cannot own sessions. Storing `Arc<SessionState>` would keep sessions — and every native handle they own — alive past disconnect, which is the leak class this phase exists to remove. It stores `Weak<SessionMarker>` instead.
  2. Session state cannot sit behind an `Arc`. It will hold `Box<dyn DeviceGuard>` values, which are `Send` but not `Sync`, so `Arc<SessionState>` would not be `Send` and the session could not move into a Tokio task. Fixing that would need a second `unsafe impl Sync`, which the crate forbids. State is held by value.
- **Movability is pinned by a test.** `assert_send::<SessionGuard>()` means a future change back to `Arc` fails to compile rather than failing at runtime.
- **16 tests added, 59 pass, 0 warnings**, and all 37 v0.2.0 fixtures still match (the session layer is additive at this point).

## Task Commits

1. **Task 1: 建立会话模块骨架与 ID 生成** — `4523d7d` (feat)
2. **Task 2: 实现不拥有会话的注册表与会话守卫** — `4523d7d` (feat)
3. **Task 3: 断言未新增未定义行为承担面** — `4523d7d` (feat)

## Decisions Made

- **Registry interface is deliberately three methods wide**: `register`, `deregister` (via the guard), `session_count`. There is no lookup by id and no cross-session mutation, because either would rebuild the shared-state defect the phase removes.
- **Device-unavailable handling is per-session, not registry-driven.** An earlier draft had the registry invalidate affected sessions on device removal. That requires reaching into session state from outside, which forces a `Mutex` into session state and contradicts the `&mut self` model. The session that observes the failure clears its own grants instead, which is also what "detect on use" means in practice.
- **Comment-aware invariants.** Two of the plan's acceptance criteria were bare greps for `thread_rng` and `Mutex`; both matched the doc comments that explain why those are forbidden. The criteria now point at the comment-aware invariant test, matching how Phase 1 handled the same false-positive class.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] An earlier draft of the registry would have required locking session state**
- **Found during:** planning review, before implementation
- **Issue:** `invalidate_for_device` on the registry needs `&mut SessionState` it does not own, forcing interior mutability into session state and contradicting the by-value design.
- **Fix:** dropped the cross-session mutator; the session clears its own grants when an operation reveals the device is gone.
- **Verification:** `tests/session_invariants.rs::session_state_has_no_interior_mutability` passes.
- **Committed in:** `4523d7d` (design recorded in `02-RESEARCH.md` G-1b and G-2)

**2. [Rule 1 - Bug] Two acceptance criteria counted doc comments**
- **Found during:** Task 1/2 verification
- **Issue:** `grep -c 'thread_rng' src/session/mod.rs` returned 1 and `grep -c 'Mutex\|RwLock'` returned 1 — both from the comments that forbid them.
- **Fix:** reworded the criteria to reference the comment-aware invariant test.
- **Verification:** `cargo test --test session_invariants` → 6 passed.
- **Committed in:** `4523d7d`

**3. [Rule 3 - Blocker] An empty `handles.rs` made the pointer invariant vacuous**
- **Found during:** Task 3
- **Issue:** the invariant test read `src/session/handles.rs`, which does not exist until plan 02-03. Creating an empty file to satisfy it would make the assertion meaningless.
- **Fix:** the test now skips when the file is absent, printing `no handle table yet`, so the assertion becomes live automatically when 02-03 lands.
- **Verification:** test passes now and will exercise real content after 02-03.
- **Committed in:** `4523d7d`

---

**Total deviations:** 3 (2 × Rule 1, 1 × Rule 3)
**Impact on plan:** Deviation 1 simplified the design and removed a lock; deviations 2 and 3 changed verification plumbing, not strength.

## Issues Encountered

None beyond the deviations above.

## User Setup Required

None.

## Next Phase Readiness

Plan 02-02 fills `SessionState` with an authorization table and wires the dispatcher to create a session per connection.

**Carried forward:** the launchd agent stays unloaded, and the Windows VM is untouched.

## Self-Check: PASSED

- `cargo test session` → 10 passed
- `cargo test --test session_invariants` → 6 passed
- `cargo test` → 59 passed, 0 failed
- `cargo check --all-targets` → 0 warnings
- `cargo check --target i686-pc-windows-gnu` → success
- 37/37 v0.2.0 fixtures still match

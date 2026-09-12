---
phase: 02-session-authorization-and-resource-ownership
plan: 03
subsystem: session
tags: [opaque-handles, resource-ownership, digest, contract-change]

requires:
  - phase: 02-01
    provides: SessionState and SessionGuard
  - phase: 02-02
    provides: authorization table wired through the dispatcher
provides:
  - src/session/handles.rs with HandleTable, ResourceKind, and fieldless HandleError
  - ConnectDev / DisConnectDev / GenerateRandom resolved through session handles
  - Digest branches session-owned, with V-1 and V-2 fixed
  - An "Intentional contract changes" section in the fixture README
affects: [02-04, phase-3 concurrency work]

tech-stack:
  added: []
  patterns:
    - "Opaque handles with a type prefix; the prefix is checked on every lookup"
    - "Fieldless error enums so a rejection cannot echo the offending value"
    - "Hand-written Debug that reports counts only"
    - "Contract changes recorded in the fixture README plus provenance, never in normalize"

key-files:
  created:
    - src/session/handles.rs
  modified:
    - src/session/mod.rs
    - src/main.rs
    - tests/fixtures/v0.2.0/DisConnectDev.json
    - tests/fixtures/v0.2.0/README.md

key-decisions:
  - "A client-supplied integer is never converted to a native handle; that conversion was the direct cause of the observed process crash"
  - "The digest rejection message is preserved byte-for-byte while the handle format changes, because only the rejection is recorded"
  - "The DisConnectDev fixture was updated as an intentional contract change, with provenance and README documenting why"
  - "DigestInit keeps a fallback path for non-default aliases, because the provider factory only builds the default"

patterns-established:
  - "Record a deliberate contract change in the fixture README and provenance; never widen normalize"
  - "Test a release guarantee by abandoning the resource rather than releasing it properly"

requirements-completed: [RES-01, RES-02, RES-03, RES-05]

duration: 90min
completed: 2026-09-11
---

# Phase 2 Plan 3: Opaque, Session-Owned Resource Handles Summary

**Clients can no longer name a native resource: every handle is a server-issued `dev-N`/`app-N`/`cnt-N`/`hsh-N` owned by one session, and abandoning a digest no longer leaks a device handle for the life of the process.**

## Performance

- **Duration:** 90 min
- **Started:** 2026-09-11T17:15:00Z
- **Completed:** 2026-09-11T18:45:00Z
- **Tasks:** 4 completed
- **Files modified:** 1 created, 4 modified

## Accomplishments

- **RES-01 satisfied.** `ConnectDev` returns a server-issued handle instead of the decimal value of a native pointer, and `DisConnectDev`/`GenerateRandom` no longer parse an integer into a `DEVHANDLE`. `grep -c 'as DEVHANDLE' src/main.rs` is **0**, down from 23. This is the defect that was observed to **kill the service process**.
- **RES-02 satisfied.** `HandleError` distinguishes `Unknown` from `WrongKind`, and the table refuses bare numbers, empty strings, unknown prefixes, and cross-kind use. A test pins `HandleError` to one byte so it cannot start carrying the input back to the client.
- **RES-03 and RES-05 satisfied.** The table is a field of `SessionState`, so it is session-scoped by construction; dropping the session drops every guard. `dropping_the_table_releases_every_resource` asserts `CloseDevice` and `CloseDigest` are recorded.
- **V-1 fixed with a regression test that could not pass before.** `digest_handle_is_released_when_the_session_ends` abandons a digest without finalising it and asserts both handles were released. Previously the handle sat in a process-global map cleaned only by `DigestFinal`/`CloseHash`.
- **V-2 fixed for the digest path.** The four digest branches no longer call `Library::new`; `Library::new` in `main.rs` fell from 11 to 8, with the remaining six belonging to Phase 3 (four) and plan 02-04 (two).
- **The frozen rejection survived the format change.** `DigestUpdate`/`DigestFinal`/`CloseHash` still answer `-11` with the exact recorded string, so changing `hash_{id}` to `hsh-N` needed no fixture edit — only the `DisConnectDev` integer case did.
- **80 tests pass, 0 warnings**, i686 cross-check clean, and 37/37 fixtures match.

## Task Commits

All four tasks landed in `fed1550`, plus the device-branch routing in `1878aac`.

## Decisions Made

- **The `DisConnectDev` fixture is a documented contract change.** It recorded the old behaviour for `params[0] = 1`: the integer became a native handle and the vendor library replied `0x0A000026 "DisControlDev failed"`. That is what RES-01 removes, so the fixture now asserts the `-11` rejection, its `provenance` carries a CONTRACT CHANGE note, and the fixture README gained an "Intentional contract changes" table. `normalize` was not touched.
- **The recording exposed an asymmetry worth keeping.** The same fabricated handle that merely errored in `DisConnectDev` killed the process in `GenerateRandom`. Relying on vendor validation of a bogus handle is not safe.
- **`HandleError` is fieldless.** A rejection that could carry the offending value would leak input back to the client and risk putting handle material in a log.
- **`Debug` for `HandleTable` is hand-written and reports counts only**, because the guard traits are not `Debug` and a handle string must not reach a log line.
- **`DigestInit` keeps an explicit fallback for non-default provider aliases.** The Phase 1 factory builds a provider only for the default alias; the fallback is commented rather than silent.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `unwrap_err()` cannot be used on handle accessors**
- **Found during:** Task 1/2 (`cargo test --lib session`)
- **Issue:** `unwrap_err()` requires the `Ok` type to be `Debug`, but the guard traits deliberately are not, so ten assertions failed to compile.
- **Fix:** added a `rejection()` helper that matches on the result without needing `Debug`.
- **Verification:** 31 session tests pass.
- **Committed in:** `1878aac`, `fed1550`

**2. [Rule 1 - Bug] `#[derive(Debug)]` on `HandleTable` did not compile, and would have been wrong anyway**
- **Found during:** Task 1
- **Issue:** the guard traits are not `Debug`, so the derive failed. Making them `Debug` would have allowed handle material into logs.
- **Fix:** hand-written `Debug` reporting only counts.
- **Verification:** compiles; `cargo test --test session_invariants` still passes.
- **Committed in:** `1878aac`

**3. [Rule 1 - Bug] An accessor landed on `SessionMarker` instead of `SessionState`**
- **Found during:** Task 1 (`cargo check`)
- **Issue:** a `str.replace` anchored on `pub fn id(&self) -> &SessionId` matched the marker type first, then a follow-up edit produced duplicate definitions.
- **Fix:** moved the accessors to `SessionState` and removed the duplicates.
- **Verification:** `grep -c 'pub fn handles'` → 2 (getter and setter).
- **Committed in:** `1878aac`

**4. [Rule 1 - Bug] Dead code and a pointless helper after the migration**
- **Found during:** Task 3 (diff review, per the session's blocking constraint)
- **Issue:** the first version of `DisConnectDev` contained a `match` with a single wildcard arm, and a `handle_rejected()` helper returned a response with `id: None` — the same anti-pattern that was fixed as M-04 in Phase 1.
- **Fix:** deleted the helper and the degenerate `match`; each rejection builds its response with the id.
- **Verification:** `grep -c 'handle_rejected'` → 0; the compiler was clean either way, which is why the diff was read hunk by hunk.
- **Committed in:** `1878aac`

**5. [Rule 1 - Bug] Acceptance criteria counted comments again**
- **Found during:** Task 4
- **Issue:** `grep -c 'hash_'` returned 23, but the matches are `hash_buf`, `hash_len`, `hash_alg_id` — local variables in the out-of-scope `Digest` branch. The real target was the key derivation.
- **Fix:** reworded the criterion to `grep -c 'format!("hash_'` (which is 0), with a note explaining the distinction.
- **Verification:** `format!("hash_` → 0; `hash_handles` → 0.
- **Committed in:** `fed1550`

---

**Total deviations:** 5 auto-fixed (5 × Rule 1)
**Impact on plan:** Deviation 4 was caught only because the diff was reviewed rather than trusted to the compiler — exactly the blocking constraint recorded at the last pause.

## Issues Encountered

- **The contract oracle did its job.** The first replay after routing the device branches failed on `DisConnectDev` with a real drift, which forced the change to be classified and documented instead of absorbed. That is the sequence the fixture discipline exists to produce.

## User Setup Required

None.

## Next Phase Readiness

Plan 02-04 extracts `protocol/`, migrates the remaining 12 security-relevant branches into `domain/`, and verifies RES-04 (release on native error paths) with fake-injected failures.

**Carried forward:**
- Six `Library::new` sites remain by design: four are Phase 3 scope, two belong to 02-04.
- 56 `CString::into_raw()` leaks remain in unmigrated branches; they fall as those branches move.
- The launchd agent stays unloaded; the Windows VM is untouched.

## Self-Check: PASSED

- `cargo test` → 60 lib + 4 contract + 4 oracle + 6 + 6 invariant = 80 passed, 0 failed
- `cargo check --all-targets` → 0 warnings
- `cargo check --target i686-pc-windows-gnu` → success
- `grep -c 'as DEVHANDLE' src/main.rs` → 0 (was 23)
- `grep -c 'hash_handles' src/main.rs` → 0; `grep -c 'format!("hash_' src/main.rs` → 0
- `grep -c 'Invalid or expired hash handle' src/main.rs` → 3
- `cargo test --test contract_fixtures` → 37 compared, 0 failures

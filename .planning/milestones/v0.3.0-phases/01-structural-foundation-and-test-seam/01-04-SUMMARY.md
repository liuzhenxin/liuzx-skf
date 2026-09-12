---
phase: 01-structural-foundation-and-test-seam
plan: 04
subsystem: server
tags: [provider-injection, ephemeral-port, contract-replay, windows-service-migration]

requires:
  - phase: 01-03
    provides: SkfProvider trait, native implementation with guards, failure-injecting fake
  - phase: 01-01
    provides: 37 frozen v0.2.0 fixtures and the normalization oracle
provides:
  - src/server/mod.rs with bind/serve/run_server, ProviderFactory injection, and resolved bind addresses
  - src/session seam keeping the dispatcher in the binary crate while the library owns transport
  - tests/contract_fixtures.rs replaying every fixture against the real binary on an ephemeral port
  - tests/common/mod.rs shared normalization for both test crates
  - win_service migrated into the library crate
affects: [phase-2 session and resource-ownership work, phase-3 concurrency work]

tech-stack:
  added: []
  patterns:
    - "bind/serve split so a caller learns the OS-chosen port before serving"
    - "SessionFactory seam: library owns transport, binary owns request semantics"
    - "Function-pointer session builder installed at startup because the SCM entry point cannot receive user data"
    - "Subprocess-based end-to-end test reading the bound port from the service's stdout"
    - "Blocking vendor calls stay on the blocking pool"

key-files:
  created:
    - src/server/mod.rs
    - tests/contract_fixtures.rs
    - tests/common/mod.rs
  modified:
    - src/main.rs
    - src/lib.rs
    - src/win_service.rs
    - tests/fixture_oracle.rs

key-decisions:
  - "WaitForDevEvent must run via spawn_blocking: on a runtime worker, a concurrent CancelWaitForDevEvent blocked as well and deadlocked the pair"
  - "Only the default provider alias is routed; a request naming another alias keeps the original direct path so behaviour cannot change for providers the factory does not build"
  - "The contract replay drives the real binary as a subprocess, because the dispatcher intentionally stays in the binary crate (D-06) and testing the shipped executable is the stronger check"
  - "WaitForDevEvent is shape-checked rather than value-compared, because its recording is timing-sensitive"

patterns-established:
  - "Server reports the resolved address, never the requested one"
  - "Provider construction failure degrades to an error response instead of process exit"
  - "Integration tests share helpers through tests/common/mod.rs rather than duplicating normalization"

requirements-completed: [FOUND-03, FOUND-01, FOUND-02]

duration: 95min
completed: 2026-09-11
---

# Phase 1 Plan 4: Provider Injection, Ephemeral-Port Server, and Contract Replay Summary

**The service is now testable end to end: it reports the port the OS actually chose, takes its provider through an injection point, and replays all 37 frozen v0.2.0 fixtures against the real binary — 42 tests pass with zero warnings.**

## Performance

- **Duration:** 95 min
- **Started:** 2026-09-11T08:25:00Z
- **Completed:** 2026-09-11T10:00:00Z
- **Tasks:** 5 completed
- **Files modified:** 3 created, 4 modified

## Accomplishments

- **The ephemeral-port limitation is gone.** `bind()` returns `listener.local_addr()`. Previously the server printed only the requested address, so `127.0.0.1:0` produced a port nobody could learn — which is why the plan's replay test could not have worked before this change.
- **FOUND-03 satisfied end to end.** `tests/contract_fixtures.rs` starts the real binary on an ephemeral port, reads the bound port from its stdout, and replays every fixture: 37 compared, 1 shape-checked, 0 failures. Plus `fixture_count_is_at_least_37`, `every_expected_method_has_a_fixture`, and a connection test.
- **Provider injection exists (D-05).** `ProviderFactory` builds the provider in one place; the server hands it to the session factory. When the default provider cannot be prepared, `UnavailableProvider` answers with the established `Load Lib Failed` contract instead of the process exiting.
- **`win_service` lives in the library crate** and the Windows target still compiles.
- **Default bind behaviour is unchanged.** The three defaults (`127.0.0.1:9001`, `0.0.0.0:8000` console, `127.0.0.1:8000` service) and all four `SKF_*` overrides are preserved, now expressed through an explicit `RunMode` instead of being inferred from `shutdown.is_some()`.
- **42 tests, 0 warnings.** `cargo check --all-targets` is clean and `cargo check --target i686-pc-windows-gnu` passes.

## Task Commits

All five tasks landed in `3fc2fa9` (feat): the server split, the `win_service` migration, the four routed branches, and the replay test are one coherent change — each is unverifiable without the others.

## Files Created/Modified

- `src/server/mod.rs` — `RunMode`, `ServerOptions`, `BoundServer`, `ProviderFactory`, `UnavailableProvider`, `Session`, `SessionFactory`, the installed session builder, `bind`, `serve`, `run_server`, and the WebSocket client loop.
- `src/main.rs` — `JsonRpcSession`/`JsonRpcSessionFactory` adapt the dispatcher to the transport seam; `build_sessions` is installed at startup; four branches routed; the old `run_server` and `spawn_ws_client` removed.
- `src/win_service.rs` — now a library module calling `crate::server::run_server`.
- `tests/contract_fixtures.rs` — subprocess replay.
- `tests/common/mod.rs` — shared normalization, also used by `tests/fixture_oracle.rs`.

## Decisions Made

- **`WaitForDevEvent` must keep using the blocking pool.** See deviations below; this is the most important finding of the plan.
- **Only the default alias is routed.** `EnumDevice`, `GetDevState`, and `WaitForDevEvent` accept a provider parameter. A request naming a non-default alias keeps the original direct path, so behaviour cannot change for providers the Phase 1 factory does not build. This is a scope limitation, documented here and in the plan.
- **The replay drives the binary as a subprocess.** The dispatcher stays in the binary crate by decision D-06, so a library integration test cannot reach it. Testing the shipped executable is also strictly stronger than testing an in-process server, and `CARGO_BIN_EXE_skf-service` makes it a supported mechanism.
- **`WaitForDevEvent` is shape-checked, not value-compared.** Its recording requires a second connection to cancel, and the cancel must arrive after the waiter entered the blocking vendor call. The reason is recorded in the fixture README and in the test itself.
- **Provider construction failure is non-fatal.** Exiting on startup would change the observable contract; the established behaviour is an error response per request.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 4 → resolved as Rule 1] Routing `WaitForDevEvent` on a runtime worker deadlocked cancellation**
- **Found during:** Task 3 (first replay attempt hung; the recorder then timed out after 900 s)
- **Issue:** The provider call was made directly in the async handler. The service stopped answering `WaitForDevEvent` entirely: `CancelWaitForDevEvent` on a second connection blocked instead of returning `true`. The pre-refactor code had wrapped this call in `tokio::task::spawn_blocking`, and that wrapper was the load-bearing part.
- **Fix:** Keep the vendor call on the blocking pool: `tokio::task::spawn_blocking(move || provider.wait_for_event(256)).await`, mapping a join failure to the original `Task panicked: {}` message.
- **Verification:** After the fix, cancel returns `true` and the waiter rejects with exactly `{"code":167772194,"message":"WaitForDevEvent failed: 0xA000022"}` — byte-identical to the recorded fixture. All 37 fixtures then matched.
- **Note:** This was treated as a correctness bug rather than a Rule 4 architectural stop: the fix restores the original threading arrangement, it does not introduce a new one.
- **Committed in:** `3fc2fa9`

**2. [Rule 1 - Bug] Arm-replacement helper duplicated the closing brace**
- **Found during:** Task 3 (first routing attempt; `cargo check` reported an unmatched delimiter)
- **Issue:** The script that replaced match arms kept the original `        },` terminator and appended a new one, unbalancing the file.
- **Fix:** Reverted `main.rs` to its committed state and re-ran the whole patch set with a corrected helper that drops the original terminator.
- **Verification:** `cargo check --all-targets` clean; the four routed branches behave identically to the recorded contract.
- **Committed in:** `3fc2fa9`

**3. [Rule 1 - Bug] Three address grep criteria counted the documenting comment**
- **Found during:** Task 5 (running the plan's criteria)
- **Issue:** `grep -c '0.0.0.0:8000' src/server/mod.rs` and its two siblings returned 2, because each literal appears both in code and in the doc comment that documents the default. Same class of false positive as in plan 01-03.
- **Fix:** Reworded to "at least 1" and added a criterion that the three defaults are preserved verbatim.
- **Verification:** `ServerOptions::from_env` is the single implementation; the console/service HTTP defaults are asserted by reading the code and by the service binding correctly in the replay test.
- **Committed in:** `3fc2fa9`

**4. [Rule 3 - Blocker] Test helper moved to a shared module**
- **Found during:** Task 4
- **Issue:** `contract_fixtures.rs` needed the same normalization as `fixture_oracle.rs`; duplicating it would let the two copies drift, which is exactly the class of problem the `<unordered>` incident exposed.
- **Fix:** Extracted `tests/common/mod.rs` and pointed both test crates at it via `mod common;`.
- **Verification:** Both `cargo test --test fixture_oracle` and `cargo test --test contract_fixtures` pass.
- **Committed in:** `3fc2fa9`

---

**Total deviations:** 4 auto-fixed (3 × Rule 1, 1 × Rule 3)
**Impact on plan:** Deviation 1 was a genuine correctness regression introduced by the routing and is now fixed with evidence. Deviation 2 was a scripting error with no lasting effect. Deviation 3 changed verification wording, not strength. Deviation 4 removed a duplication risk. No scope creep: no unplanned method was routed, and no error code or message changed.

## Issues Encountered

- **FOUND-04 is satisfied in the "abstraction exists and is complete" sense, not "all production paths route through it".** The trait covers every operation group and the fake can inject all five failure kinds, but by decision D-06 only four branches are routed; the other 33 still call `SkfApi` directly. `src/main.rs` therefore still contains 11 `Library::new` sites. This is recorded in `01-VALIDATION.md` and must not be reported as fully routed.
- **The recording environment is now part of the contract.** `EnumDevice` returns a middleware-reported device name (`BE40503E6E5D8F491AF943EDE321192`). It is stable across processes on this machine, but a different environment would produce a different value and the replay would fail — correctly, since the fixture asserts the recorded contract. A machine change requires re-recording.

## User Setup Required

None.

## Next Phase Readiness

All four Phase 1 plans are complete. The remaining phase-level work is verification and the security gate; the `com.liuzx.skf-service` launchd agent still needs reloading after the phase.

**Carried forward to Phase 2:**
- 33 branches still call `SkfApi` directly and still load the library per call; the provider seam is ready for them.
- `ProviderFactory` builds only the default alias; per-alias providers are a Phase 2 concern once sessions own provider selection.
- `Session` is the natural home for the session state Phase 2 introduces (SESS-01..06, RES-01..05).

## Self-Check: PASSED

- `cargo test` → 28 lib + 4 oracle + 4 contract + 6 invariant = 42 passed, 0 failed
- `cargo test --test contract_fixtures` → 37 compared, 1 shape-checked, 0 failures
- `cargo check --all-targets` → 0 warnings
- `cargo check --target i686-pc-windows-gnu` → success
- `grep -c 'local_addr()' src/server/mod.rs` → 3
- `grep -c 'mod win_service' src/main.rs` → 0; `grep -c 'cfg(windows)' src/lib.rs` → 1
- `grep -c 'ctx.provider' src/main.rs` → 4 (the four routed branches)
- Behaviour preservation: all 37 fixtures MATCH

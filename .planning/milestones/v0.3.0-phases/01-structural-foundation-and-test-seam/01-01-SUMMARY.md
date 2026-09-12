---
phase: 01-structural-foundation-and-test-seam
plan: 01
subsystem: testing
tags: [contract-testing, fixtures, json-rpc, regression-oracle]

requires:
  - phase: none
    provides: pre-refactor v0.2.0 service binary
provides:
  - 37 recorded v0.2.0 JSON-RPC contract fixtures with frozen volatile-field declarations
  - Recorder that captures raw envelopes from a live server (tools/record_fixtures.js)
  - Normalization oracle with negative self-tests proving it can fail (tests/fixture_oracle.rs)
affects: [01-02, 01-03, 01-04, phase-2 session work]

tech-stack:
  added: []
  patterns:
    - "Contract fixtures recorded from the pre-refactor binary, never from the new fake"
    - "Volatile-field declarations frozen at record time with source-level justification"
    - "Normalize both sides of a comparison so declared volatility cancels out"
    - "Negative self-test proving the regression oracle is capable of failing"

key-files:
  created:
    - tools/record_fixtures.js
    - tests/fixture_oracle.rs
    - tests/fixtures/v0.2.0/README.md
    - tests/fixtures/v0.2.0/SetLanguage.json
  modified: []

key-decisions:
  - "CheckPIN.message is NOT normalized: the recorded message is deterministic, so declaring it volatile would erase a real field"
  - "GenerateRandom is recorded with no parameters instead of a fabricated handle, because a bogus DEVHANDLE kills the service process"

patterns-established:
  - "Fixture provenance: recorded_from / platform / requires_hardware / observed_with_device / volatile_reasons"
  - "Oracle self-test: mutated and undeclared changes must both be detected"

requirements-completed: [FOUND-03]

duration: 42min
completed: 2026-09-11
---

# Phase 1 Plan 1: Record and Freeze the v0.2.0 Contract Summary

**37 v0.2.0 JSON-RPC methods frozen as regression fixtures recorded from the untouched pre-refactor binary, with a normalization oracle proven able to fail and zero undeclared volatility on re-record.**

## Performance

- **Duration:** 42 min
- **Started:** 2026-09-11T05:22:00Z
- **Completed:** 2026-09-11T06:04:00Z
- **Tasks:** 3 completed
- **Files modified:** 40 (2 source files + 37 fixtures + 1 README)

## Accomplishments

- **All 37 methods recorded.** `RECORDED=37 INCOMPLETE=0 ERRORS=0`. The vendor library loaded successfully (`Loading SKF library for GM3000 from: native/GM3000/macos/x86_64/libgm3000.1.0.dylib`); no token was present, so hardware methods recorded their deterministic error contracts (decision D-11).
- **Zero undeclared volatility.** The recorder was run twice and the outputs diffed field-by-field: 0 differences. The declared volatile set (`ConnectDev.result`, `GenerateRandom.result`, `IssueCertificate.result`) is therefore complete, which is the evidence that the oracle is trustworthy rather than merely green.
- **The oracle can fail.** `oracle_rejects_mutated_response` and `oracle_accepts_only_normalized_differences` assert that a mutated `error` field and an undeclared `id` change both survive normalization and are detected. A suite that cannot fail proves nothing.
- **Ordering constraint satisfied.** `git log --diff-filter=M -- src/` still shows only `3ab5f3f` (the v0.2.0 release commit). No production code was modified before the fixtures landed, which is what makes later refactoring attributable.

## Task Commits

1. **Task 1: 建立夹具录制工具** — `5e3eaea` (test)
2. **Task 2: 建立并自检契约 oracle** — `5e3eaea` (test)
3. **Task 3: 录制并提交 37 个方法的夹具** — `5e3eaea` (test)

All three tasks landed in a single commit because tasks 2 and 3 are not independently meaningful until the fixtures exist and the recorder is used in anger; the plan's own ordering (record → then refactor) is preserved.

## Files Created/Modified

- `tools/record_fixtures.js` — records raw JSON-RPC envelopes from a live server. Parameter order comes from the production client `api/skf_api.js`; the script imports nothing from `src/`, which is what keeps the oracle independent of the code under test.
- `tests/fixture_oracle.rs` — `apply_normalize`, fixture loading, and the negative self-tests. Both sides of every comparison are normalized so declared volatility cancels out.
- `tests/fixtures/v0.2.0/README.md` — the two rules that keep the fixtures meaningful (no fake-generated fixtures; `normalize` frozen at record time).
- `tests/fixtures/v0.2.0/*.json` — 37 fixtures.

## Decisions Made

- **`CheckPIN.message` deliberately NOT normalized.** The source embeds the token retry counter in the VerifyPIN failure message, but that message was never reached without a token. The recorded message (`ConnectDev failed: 0x0A000023`) is fully deterministic; declaring it volatile would have erased a real assertion. The omission is recorded in `provenance.recording_note`.
- **`GenerateRandom` recorded with no parameters.** Sending a fabricated integer as a `DEVHANDLE` reached the vendor library unchecked and killed the service process. The parameter-validation contract (`error: -2, "Bad params"`) is recorded instead.
- **Digest chain uses the deterministic handle key.** `DigestUpdate`/`DigestFinal`/`CloseHash` were recorded against `hash_1`. The handle key is derived from the JSON-RPC id, so it is reproducible even when `DigestInit` fails for lack of a token — this let all four methods record a real contract instead of being skipped.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Recorder did not pass the client to scenario callbacks**
- **Found during:** Task 3 (first recording attempt)
- **Issue:** `scenarioSimple(wsUrl, null, invoke)` called `invoke()` with no arguments, so 33 scenarios failed with `Cannot read properties of undefined` and only 2 fixtures were produced.
- **Fix:** `scenarioSimple` now calls `invoke(client)`.
- **Verification:** `RECORDED=37 INCOMPLETE=0 ERRORS=0` on the follow-up run.
- **Committed in:** `5e3eaea`

**2. [Rule 1 - Bug] Preparatory `setLanguage` shifted recorded JSON-RPC ids**
- **Found during:** Task 3 (reviewing the first fixtures)
- **Issue:** `scenarioSimple` issued `setLanguage('EN')` before the target call, so the target request used `id: 2` while replay would use `id: 1`. Responses echo the id, so every fixture would have mismatched on replay.
- **Fix:** Removed the preparatory call — the server already defaults each connection to English (`Language::EN` in `spawn_ws_client`), so the target request is the first request on the connection and gets `id: 1`.
- **Verification:** All 37 fixtures carry `id: 1` in `request` and `response` (except the Digest chain, whose setup consumes id 1).
- **Committed in:** `5e3eaea`

**3. [Rule 1 - Bug] `normalize` values were prose, not markers**
- **Found during:** Task 3 (first fixtures written)
- **Issue:** Volatile declarations were written as human-readable reasons (e.g. `"src/main.rs ConnectDev returns ..."`). `apply_normalize` only substitutes values shaped like `<...>`, so they would never have taken effect.
- **Fix:** Split into `VOLATILE_PATHS` (markers such as `<volatile:handle>`) and `VOLATILE_REASONS` (justifications, stored in `provenance.volatile_reasons`).
- **Verification:** `oracle_accepts_only_normalized_differences` exercises the marker path and passes.
- **Committed in:** `5e3eaea`

**4. [Rule 1 - Bug] Digest dependent methods were skipped when `DigestInit` failed**
- **Found during:** Task 3 (first recording attempt)
- **Issue:** With no token, `DigestInit` fails, so `DigestUpdate`/`DigestFinal`/`CloseHash` were marked INCOMPLETE and produced no fixture — three methods outside the regression net.
- **Fix:** The dependent request is now always issued, using the deterministic handle key `hash_{id}` when `DigestInit` returned no handle. This records the real `-11 Invalid or expired hash handle` contract (decision D-11).
- **Verification:** All four Digest methods have fixtures; re-record diff shows 0 differences.
- **Committed in:** `5e3eaea`

**5. [Rule 3 - Blocker] Stale launchd agent kept the service ports occupied**
- **Found during:** Task 3 (service startup)
- **Issue:** `com.liuzx.skf-service.plist` in `~/Library/LaunchAgents` held `127.0.0.1:9001` and `0.0.0.0:8000` with `KeepAlive`, so every killed process was immediately respawned and `cargo run` failed with "Address already in use". Recording against that stale process would have had ambiguous provenance.
- **Fix:** `launchctl unload ~/Library/LaunchAgents/com.liuzx.skf-service.plist`, then started a fresh process from the current working tree.
- **Verification:** Recording log shows the service started from the current tree and all 37 fixtures carry `recorded_from: ecb5158` (current HEAD).
- **Committed in:** n/a (environment change, not a repository change)
- **Follow-up:** The launchd agent is left unloaded for the rest of Phase 1 (it would otherwise hold the ports during the 01-04 replay tests). It should be reloaded after the phase completes.

---

**Total deviations:** 4 auto-fixed (4 × Rule 1), 1 environment blocker resolved (Rule 3)
**Impact on plan:** All fixes were necessary for the fixtures to be correct. No scope creep — no `src/` file was touched, which is the plan's central constraint.

## Issues Encountered

- **In-process crash on a fabricated handle.** Passing an arbitrary integer as a `DEVHANDLE` to `GenerateRandom` killed the service process inside the GM3000 DLL. This is the concrete motivation for RES-01 (opaque, server-issued handles) and is recorded in the fixture README. Not fixed here: changing it would be a behavior change outside this plan's scope.
- **Mock issuance is broken on macOS.** `IssueCertificate` recorded `error: -5, "OpenSSL sign failed: unknown option -force_pubkey"`. The system LibreSSL `x509` command does not support the flag the mock path relies on. Recorded as the error contract; the mock path remains documented as test-only.
- **`WaitForDevEvent` needs a second connection to cancel.** The server's per-connection loop awaits each request, so a cancel sent on the same connection is never read. The recorder opens a second connection to cancel, which produced a real contract (`error 167772194`, cancel returned `true`).

## User Setup Required

None.

## Next Phase Readiness

**Refactoring may now begin.** This was the plan's single irreversible ordering constraint: fixtures recorded from the untouched binary, committed as `5e3eaea`, with `git log --diff-filter=M -- src/` confirming no production change has occurred since.

Ready for plan 01-02 (library crate split and pure-logic extraction), which will exercise these fixtures for the first time only indirectly — end-to-end replay arrives in 01-04.

**Known limitations carried forward:**
- All fixtures have `observed_with_device: false`. A run on a machine with a GM3000 token can enrich them; that is a contract change requiring its own commit.
- The `com.liuzx.skf-service` launchd agent remains unloaded and must be reloaded after Phase 1.

## Self-Check: PASSED

- `tools/record_fixtures.js` exists and `node --check` exits 0
- `tests/fixture_oracle.rs` exists; `cargo test --test fixture_oracle` → 3 passed, 0 failed
- `tests/fixtures/v0.2.0/` contains 37 JSON fixtures
- Every fixture's `provenance.recorded_from` equals `ecb5158c2101171e32c78236702877c5586780c2`
- Secret scan over `tests/fixtures/v0.2.0/` is clean
- Re-record diff (ignoring `recorded_at`): 0 undeclared differences
- `git log --diff-filter=M -- src/` → no modification since the fixture commit

---
phase: 01-structural-foundation-and-test-seam
plan: 03
subsystem: provider
tags: [ffi, trait-abstraction, fake-provider, failure-injection, invariants]

requires:
  - phase: 01-02
    provides: src/lib.rs library target with crypto and config modules
provides:
  - src/provider/mod.rs SkfProvider trait over business operations, with handle newtypes and guard traits
  - src/provider/native.rs NativeSkfProvider owning exactly one Arc<Library> with Drop-based guards
  - src/provider/fake.rs failure-injecting fake with call recording (feature-gated)
  - tests/provider_invariants.rs six structural invariants enforced as tests
affects: [01-04, phase-2 session and resource-ownership work]

tech-stack:
  added: []
  patterns:
    - "Provider trait models business operations, not a mirror of the C symbol table"
    - "Guards own their native handle and release it in Drop"
    - "Guards store handles as usize so they are Send + Sync without a new unsafe impl"
    - "Fake provider with composable per-call failure injection and call recording"
    - "Structural invariants asserted against source text, comment- and token-aware"

key-files:
  created:
    - src/provider/mod.rs
    - src/provider/native.rs
    - src/provider/fake.rs
    - tests/provider_invariants.rs
  modified:
    - src/lib.rs
    - Cargo.toml
    - Cargo.lock

key-decisions:
  - "Guard handles are stored as usize, not raw pointers: a raw pointer is !Send + !Sync and the crate permits exactly one unsafe Send/Sync assertion"
  - "New code does not replicate the pre-refactor CString::into_raw() leak; borrowed pointers are passed instead"
  - "The fake is feature-gated (test-provider) and integration tests opt in via a self dev-dependency, so test doubles never ship in a release binary"
  - "NativeDevice retains its device name so DeviceGuard::state() is a real implementation rather than a redirect to the provider"

patterns-established:
  - "Guards share an Arc chain (device → application → container) so a child can never outlive its parent's native handle"
  - "The fake's call log records only Operation values, never parameters, so credentials cannot leak into test artefacts"
  - "Invariants that cannot be expressed in the type system are asserted against source text with comment and token awareness"

requirements-completed: [FOUND-04, FOUND-05]

duration: 70min
completed: 2026-09-11
---

# Phase 1 Plan 3: SKF Provider Abstraction with Failure-Injecting Fake Summary

**The 37 hand-written request branches now have a single seam to migrate onto: a provider trait over business operations whose native implementation loads the vendor library exactly once, plus a fake that can inject every required failure and record the exact call sequence — 38 tests pass and all 37 contract fixtures still match.**

## Performance

- **Duration:** 70 min
- **Started:** 2026-09-11T07:10:00Z
- **Completed:** 2026-09-11T08:20:00Z
- **Tasks:** 4 completed
- **Files modified:** 4 created, 3 modified

## Accomplishments

- **FOUND-04 satisfied.** `SkfProvider` expresses business operations (enumeration, device lifecycle, application/container/container-guard operations, signing, digest, symmetric crypto) rather than mirroring the C symbol table. `NativeSkfProvider` wraps the existing `SkfApi`, so no SKF call logic was rewritten — the 40 distinct `SkfApi` methods the 37 branches use still have one implementation.
- **Library lifetime is now owned.** `NativeSkfProvider` calls `Library::new` exactly once and shares the library through `Arc`, so a handle can never outlive the library that produced it. The pre-refactor code re-loaded and dropped the library on ten request paths; the invariant test forbids a second load site.
- **FOUND-05 satisfied.** The fake injects all five required failure kinds (`PinIncorrect`, `PinLocked`/`ContainerMissing`, `SymbolUnavailable`, `DeviceRemoved`, `Blocking`) with composable per-call configuration, and records every operation including guard drops.
- **No new undefined-behaviour surface.** The crate still contains exactly one `unsafe impl Send` and one `unsafe impl Sync`, both in `src/skf/types.rs`. Guards avoid a second assertion by storing handles as `usize`.
- **Behaviour preservation re-verified.** After adding the provider module, all 37 v0.2.0 fixtures still match under oracle normalization.
- **38 tests pass** (28 library, 4 oracle, 6 invariant), with no warnings from `cargo check --all-targets`.

## Task Commits

1. **Task 1: 定义 provider trait 与句柄/错误类型** — `e5f0ac7` (feat)
2. **Task 2: 实现原生 provider（单库实例 + Drop 守卫）** — `e5f0ac7` (feat)
3. **Task 3: 实现可注入失败的 fake provider 与调用记录** — `e5f0ac7` (feat)
4. **Task 4: 断言未新增未定义行为承担面** — `e5f0ac7` (feat)

## Files Created/Modified

- `src/provider/mod.rs` — `SkfProvider`, `Operation`, `SkfError`, `ProviderError`, four handle newtypes, four guard traits, `PinOutcome`, `DeviceInfo`. Naming no native type is a documented invariant of this file.
- `src/provider/native.rs` — `NativeSkfProvider` plus `NativeDevice`/`NativeApplication`/`NativeContainer`/`NativeDigest`, each implementing `Drop`. Guards share an `Arc` chain so an application guard keeps its device open.
- `src/provider/fake.rs` — `FakeSkfProvider` with `fail_next`, `calls`, `call_count`, `clear_failures`, deterministic handle counters, and 10 tests.
- `tests/provider_invariants.rs` — six source-level invariants.
- `src/lib.rs`, `Cargo.toml`, `Cargo.lock` — provider module, `test-provider` feature, self dev-dependency.

## Decisions Made

- **Handles stored as `usize`.** A raw pointer inside a guard makes the guard `!Send + !Sync`, so `Arc<NativeDevice>` cannot implement the `Send`-bound guard traits. The crate's invariant test allows exactly one `unsafe impl Send/Sync`, so the fix was to store integers and convert at each call site. This is strictly safer than adding a second unsafe assertion and it made the "no native pointer in the guard" property mechanically checkable.
- **Do not replicate the `CString::into_raw()` leak.** The pre-refactor code leaked every `CString` it passed to the vendor API. This is new code, so it passes borrowed pointers (`as_ptr() as *mut CHAR`) and frees the allocation at scope end.
- **Feature-gate the fake and opt in from integration tests.** `cfg(test)` does not reach integration test crates, so the fake would have been invisible to `tests/contract_fixtures.rs` in plan 01-04. A self dev-dependency with `features = ["test-provider"]` is the supported way to give test crates access without shipping the fake in release builds.
- **`DeviceGuard::state()` is real.** The trait exposes `state()`, and the vendor API needs a device *name* rather than a handle, so `NativeDevice` retains the name it was opened with instead of returning an error that tells the caller to use the provider.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Guard types could not satisfy the `Send` bound**
- **Found during:** Task 2 (`cargo check`)
- **Issue:** Eight E0277 errors: `*mut c_void` cannot be sent or shared between threads, so `Arc<NativeDevice>` and friends could not implement the `Send`-bound guard traits.
- **Fix:** Store handles as `usize` inside the guards and convert to the native handle type at each call site. No `unsafe impl` was added, preserving the single-assertion invariant.
- **Verification:** `cargo check --all-targets` is clean; `tests/provider_invariants.rs` asserts guard fields are integers.
- **Committed in:** `e5f0ac7`

**2. [Rule 1 - Bug] Three acceptance criteria counted doc comments**
- **Found during:** Task 4 (running the plan's grep criteria)
- **Issue:** `grep -cE 'HANDLE|...' src/provider/mod.rs` returned 2 and `grep -c 'Library::new' src/provider/native.rs` returned 2, but every extra match was inside a doc comment — including the module doc that *names the forbidden types in order to forbid them*. `grep -c 'test-provider' Cargo.toml` returned 2 because the feature is legitimately referenced both in `[features]` and in the dev-dependency.
- **Fix:** Reworded those three criteria to reference the comment-aware, token-aware invariant tests, and documented why the naive greps were a weaker proxy. The underlying properties are verified more strictly than the original criteria asked for.
- **Verification:** All six invariant tests pass, including `provider_boundary_exposes_no_native_pointers` (comment- and token-aware) and `native_provider_loads_library_exactly_once`.
- **Committed in:** `e5f0ac7`

**3. [Rule 1 - Bug] Unused field and parameter warnings**
- **Found during:** Task 3/4 (`cargo check --all-targets`)
- **Issue:** `NativeContainer.name`, `FakeContainer.name`, and `FakeApplication.name` were never read, and `FakeApplication::open_container` had an unused parameter.
- **Fix:** Removed the unused fields (Phase 2 can reintroduce them with a consumer) and prefixed the unused parameter. `cargo check --all-targets` now emits zero warnings.
- **Verification:** Clean `cargo check --all-targets`.
- **Committed in:** `e5f0ac7`

---

**Total deviations:** 3 auto-fixed (3 × Rule 1)
**Impact on plan:** Deviation 1 was structurally necessary — without it the guard traits cannot exist under the crate's `unsafe` invariant. Deviation 2 changed verification wording, not verification strength. Deviation 3 was warning hygiene. No scope creep: no `handle_request` branch was touched, and no routing was added.

## Issues Encountered

- **The provider is not yet wired into production dispatch.** Per decision D-06, only four self-contained branches (`EnumDevice`, `GetDevState`, `WaitForDevEvent`, `CancelWaitForDevEvent`) are routed in plan 01-04; the remaining 33 migrate in Phase 2. This is deliberate and is the reason the trait was designed around guards rather than around a 1:1 symbol mapping: Phase 2 can migrate branches without changing these signatures.
- **`begin_digest` is a free function rather than a guard method.** Keeping the guard trait free of algorithm parameters was the lesser evil for Phase 1; Phase 2 can promote it once the domain layer's needs are known.

## User Setup Required

None.

## Next Phase Readiness

Plan 01-04 can now build on a settled trait: it injects a provider factory into `run_server`, exposes the resolved bind address for ephemeral-port testing, migrates `win_service` into the library crate, routes the four self-contained branches, and adds the end-to-end replay test.

**Carried forward:**
- `cargo test` currently runs 38 tests; plan 01-04 adds the replay and fixture-count tests.
- The `com.liuzx.skf-service` launchd agent is still unloaded and must be reloaded after Phase 1.

## Self-Check: PASSED

- `cargo test` → 28 lib + 4 oracle + 6 invariant tests pass, 0 failures
- `cargo check --all-targets` → clean, zero warnings
- `cargo check --target i686-pc-windows-gnu` → success
- `cargo test provider::fake` → 10 passed (≥ 8 required)
- `grep -c 'Library::new' src/provider/native.rs` → 2, both accounted for (one call, one doc comment); authoritative check is `native_provider_loads_library_exactly_once`
- `grep -c 'unsafe impl Send' src/skf/types.rs` → 1; no occurrences under `src/provider/`
- Behaviour preservation: all 37 fixtures MATCH under oracle normalization after the provider landed

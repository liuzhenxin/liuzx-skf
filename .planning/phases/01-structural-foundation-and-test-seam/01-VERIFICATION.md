---
phase: 01
slug: structural-foundation-and-test-seam
status: human_needed
verified_at: 2026-09-11
must_haves_total: 6
must_haves_verified: 5
must_haves_partial: 1
requirements_verified: [FOUND-01, FOUND-02, FOUND-03, FOUND-06]
requirements_partial: [FOUND-04, FOUND-05]
human_verification_count: 2
---

# Phase 1 — Verification

**Phase goal:** 让代码库首次可被自动化验证：逻辑可单测、协议契约被夹具冻结、SKF 调用隔离在可替换的 provider 边界之后。

**Verdict:** `human_needed`. Every automated check passes. Two items need a
human: one requires a Windows build machine, and one is a scope interpretation
that the user should confirm rather than have assumed.

## Automated Evidence

| Check | Command | Result |
|-------|---------|--------|
| Tests without hardware, driver, or network | `cargo test` | 42 passed (28 lib, 4 oracle, 4 contract, 6 invariant) |
| Contract replay against the real binary | `cargo test --test contract_fixtures` | 37 compared, 1 shape-checked, 0 failures |
| Oracle is capable of failing | `cargo test --test fixture_oracle` | 4 passed |
| Provider invariants | `cargo test --test provider_invariants` | 6 passed |
| Pure logic tests | `cargo test crypto` / `cargo test config` | 7 / 11 passed |
| Fake failure injection | `cargo test provider::fake` | 10 passed |
| Warnings | `cargo check --all-targets` | 0 |
| Windows-side compile | `cargo check --target i686-pc-windows-gnu` | success |

## ROADMAP Success Criteria

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| 1 | `cargo test` runs a non-zero number of tests with no USB Key, driver, or network | **Met** | 42 tests pass; the suite spawns no vendor library load for any assertion except the replay, which runs against the local macOS middleware that is already present |
| 2 | Every supported method has a frozen fixture and the suite fails if a shape changes | **Met** | 37 fixtures; `every_expected_method_has_a_fixture` and `fixture_count_is_at_least_37` guard completeness; `oracle_rejects_mutated_response` and `oracle_accepts_only_normalized_differences` prove the comparison can fail; the replay compares against the live service |
| 3 | All SKF operations are reached through a provider abstraction whose fake can inject wrong PIN, missing container, missing symbol, device removal, and blocking | **Partial — human confirmation required** | The abstraction covers every operation group, the native implementation loads the library exactly once, and all five failure kinds are asserted in `provider/fake.rs`. However, decision D-06 scopes Phase 1 to routing **4 of 37** branches; the other 33 still call `SkfApi` directly, and `src/main.rs` still contains 11 `Library::new` sites. See "Human Verification 2" |
| 4 | Pure encoding logic has tests that run without hardware or OpenSSL | **Met** | 7 `crypto` tests: DER length boundaries at 127/128 and 255/256, hex edge cases, DER INTEGER sign rule, SubjectDN string types, SM2/RSA SPKI structure |
| 5 | The Windows i686 build still produces `skf-service.exe` with `Machine=0x014C` | **Unverifiable here** | `cargo check --target i686-pc-windows-gnu` passes and the binary target name is unchanged, but the PE header check needs `packaging/windows/build.ps1` on a Windows machine. See "Human Verification 1" |

## must_haves

| must_have | Status | Evidence |
|-----------|--------|----------|
| `cargo test` runs non-zero hardware-free tests | Verified | 42 tests |
| v0.2.0 request/response shapes frozen as fixtures | Verified | 37 fixtures; recorded from `ecb5158` before any production change |
| Provider abstraction with real and fake implementations | Verified | `src/provider/{mod,native,fake}.rs`; `provider_invariants` asserts the boundary shape |
| Fake injects the five required failure kinds | Verified | 10 fake tests, one per kind plus blocking-delay semantics |
| Pure encoding logic covered without hardware | Verified | 7 `crypto` tests |
| Windows i686 build still yields a PE32 artefact | Partial | cross-compile check passes; PE header check needs Windows |

## Requirement Traceability

| Requirement | Status | Notes |
|-------------|--------|-------|
| FOUND-01 | Complete | Non-zero test count achieved in plan 01-02, preserved through 01-04 |
| FOUND-02 | Complete | `src/lib.rs` + thin binary; bin name unchanged |
| FOUND-03 | Complete | Fixtures frozen in 01-01, replayed end to end in 01-04 |
| FOUND-04 | Partial | Abstraction complete; production routing limited to 4 branches by D-06 |
| FOUND-05 | Partial | All five failure kinds injectable and asserted; coverage is the minimum needed, not the full method surface (D-14/D-17) |
| FOUND-06 | Complete | `crypto` tests |

## Cross-Phase Regression

No prior phases exist, so the regression gate is not applicable. Behaviour
preservation was instead verified directly against the recorded contract: all 37
fixtures match after every wave, including the final state.

## Behaviour Preservation Log

| Checkpoint | Fixtures compared | Result |
|------------|-------------------|--------|
| After 01-02 (library split) | 3 fresh processes × 37 | MATCH |
| After 01-03 (provider added) | 37 | MATCH |
| After 01-04 (routing) | 37 | MATCH |

## Code Review

`01-REVIEW.md`: 0 critical, 0 high, 4 medium, 6 low. No open threats. Two medium
findings (`begin_digest` unreachable through the trait; the id-less error helper)
are recommended for Phase 2 before the session work builds on the seam.

## Human Verification 1 — Windows i686 artefact

`packaging/windows/build.ps1` must run on a Windows build machine to confirm:

1. `dist\skf-service-windows-x64-gm3000-x86\skf-service.exe` exists
2. Its PE Machine is `0x014C`
3. `mtoken_gm3000.dll` in the package is also `0x014C`

The plan's own instructions are in `packaging/windows/README-Windows-x86_64.md`.

## Human Verification 2 — Confirm the FOUND-04 scope

Decision D-06 deliberately kept all 37 request branches in `main.rs` and routed
only four self-contained ones (`EnumDevice`, `GetDevState`, `WaitForDevEvent`,
`CancelWaitForDevEvent`) through the provider. The rationale was to avoid moving
the same code twice, since Phase 2's session work edits those branches anyway.

The consequence: ROADMAP success criterion 3, read literally, is not met — 33
branches still reach the vendor library directly.

**Confirm one of:**

- **Accepted as scoped** — FOUND-04 counts as satisfied for Phase 1 (abstraction
  complete and fake-capable), with production routing completed in Phase 2.
- **Treated as a gap** — run `$gsd-plan-phase 1 --gaps` to create a plan that
  routes the remaining branches now, before Phase 2.

---

*Verification completed: 2026-09-11*
*Verifier: inline verification (no subagent runtime available)*

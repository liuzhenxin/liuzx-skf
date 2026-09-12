---
phase: 03-transport-hardening-and-concurrency
plan: 04
subsystem: domain
tags: [classification, trans-07, documentation]

requires:
  - phase: 03-01
    provides: bounded transport
provides:
  - OperationClass enum and CLASSIFIED_METHODS covering all 37 methods
  - classification coverage invariant test
  - docs/OPERATION-CLASSIFICATION.md
affects: [phase-6]

tech-stack:
  added: []
  patterns:
    - "Code is the classification source of truth; docs and tests render/validate it"

key-files:
  created:
    - src/domain/classification.rs
    - tests/classification_invariants.rs
    - docs/OPERATION-CLASSIFICATION.md
  modified:
    - src/domain/mod.rs

key-decisions:
  - "Digest lifecycle is ReadOnly: session-scoped streaming state, not persisted device state"
  - "The classification is not enforced on any request path this phase"

requirements-completed: [TRANS-07]

duration: 25min
completed: 2026-09-11
---

# Phase 3 Plan 4: Operation Classification Summary

**All 37 methods are classified ReadOnly (23) or Destructive (14), the set is verified against `EXPECTED_METHODS`, and the table is documented; nothing reads the classification at runtime.**

## Evidence

- `cargo test classification` → 2 passed.
- `cargo test --test classification_invariants` → 4 passed:
  - every expected method is classified;
  - no method is classified twice;
  - the classified set equals `EXPECTED_METHODS` (no extras, same size);
  - the documented destructive methods (`DeleteContainer`, `ImportCertificate`, `LockDev`, `SetLabel`, `CheckPIN`, `GenECCKeyPair`) are Destructive.
- `docs/OPERATION-CLASSIFICATION.md` → 37 method rows plus header; 17 `Destructive` mentions (14 rows + rule/count text).
- `cargo test` → all green; `cargo check --all-targets` → 0 warnings.

## Notes

- The rule was tightened during execution: "open or close a device, application, or container resource" rather than any native resource, because the digest lifecycle is session-scoped streaming state and the locked decision (D-20) classifies it ReadOnly. Both the module doc and the table state this explicitly.
- No request path consults `OperationClass`; enforcement (authorization, audit, confirmation) is deferred as decided.

## Self-Check: PASSED

- `grep -c 'pub enum OperationClass' src/domain/classification.rs` → 1
- `grep -c 'CLASSIFIED_METHODS' src/domain/classification.rs` → 3
- `grep -c '^| [A-Z]' docs/OPERATION-CLASSIFICATION.md` → 38 (header + 37 methods)

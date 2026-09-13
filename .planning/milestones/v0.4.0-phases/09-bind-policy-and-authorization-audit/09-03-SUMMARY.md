---
plan: 09-03
phase: 9
status: complete
completed: 2026-09-12
requirements: [AUD-01, AUD-02, BIND-01]
---

# Plan 09-03 Summary: Audit Proof and Regression

## What shipped

- `tests/audit.rs` installs a capturing `log::Log` and drives the real
  `SessionState` decision point:
  - `every_authorization_decision_is_audited` covers granted, denied (with
    `not_authorized`), expired, and device.unavailable (with `cleared:1`).
  - `audit_records_never_carry_secrets` asserts no record contains
    pin/private_key/payload/token/bearer and that each is a single line.

## Deviation

The plan called for a real-binary stderr capture. That is not reachable without a
USB token: every handler opens the device *before* it checks authorization, so an
absent token fails earlier and no deny decision occurs. The test therefore drives
the same `audit::emit` path at the library layer (its one caller is
`SessionState`, which is the code the dispatcher uses). Documented here and in
`09-VERIFICATION.md`.

## Evidence

| Check | Result |
|-------|--------|
| `cargo test --test audit` | 2 passed |
| `cargo test --no-fail-fast` | **all suites green**, including `contract_fixtures` 4/0 |
| `cargo fmt --all -- --check` / `clippy -D warnings` | clean |
| `cargo check --target i686-pc-windows-gnu` | pass |

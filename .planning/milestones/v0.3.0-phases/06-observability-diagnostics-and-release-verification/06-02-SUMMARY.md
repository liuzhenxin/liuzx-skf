---
phase: 06-observability-diagnostics-and-release-verification
plan: 02
subsystem: diagnostic
tags: [obs-03, obs-04, rel-01, rel-02, diagnostics, protocol-version]

requires: [06-01]
provides:
  - diagnose subcommand (all platforms)
  - apiVersion handling and GetProtocolVersion
  - restricted-method gate
  - started_at on the status file
affects: [06-03]

tech-stack:
  added: []
  patterns:
    - "Diagnostic output is an allowlist of non-sensitive fields; the library path never appears"

key-files:
  created:
    - src/diagnostic.rs
    - tests/protocol_version.rs
    - tests/restricted_methods.rs
  modified:
    - src/service_state.rs
    - src/win_service.rs
    - src/protocol/mod.rs
    - src/domain/classification.rs
    - src/main.rs
    - src/lib.rs
    - tests/common/mod.rs
    - tests/contract_fixtures.rs
    - tests/fixtures/v0.2.0/README.md
    - Cargo.toml
    - Cargo.lock

key-decisions:
  - "diagnose reports only booleans/alias/ports/uptime/error-class; the resolved library path is never printed"
  - "GetProtocolVersion is an explicit additive contract method (ADDITIVE_METHODS), not a v0.2.0 fixture"
  - "Restricted methods are inert unless SKF_RESTRICT_LEGACY=1, preserving the 37 fixtures"

requirements-completed: [OBS-03, OBS-04, REL-01, REL-02]

duration: 80min
completed: 2026-09-11
---

# Phase 6 Plan 2: Diagnostics, Protocol Version, and Restricted Methods Summary

**`skf-service diagnose` reports the five startup stages without leaking paths, the protocol accepts an optional version and answers `GetProtocolVersion`, and two safety-restricted methods can be disabled without changing the frozen contract.**

## What changed

| Piece | Behaviour |
|-------|-----------|
| `diagnose [--config <path>] [--json]` | config/provider/library-file/library-load/listener booleans + alias/ports/error-class + state/stage/code/address/uptime; never a path |
| `ServiceStatusFile.started_at` | epoch seconds of the first `StartPending`, preserved across states |
| `RpcRequest.api_version` | optional `apiVersion`; absent = v0.2.0 = 1; `>1` → `-1` documented |
| `GetProtocolVersion` | `{"min":1,"current":1,"service":"0.3.0"}`; listed in `ADDITIVE_METHODS` |
| `classification::restricted_rejection` | `{IssueCertificate, Transmit}` → `-100` only when `SKF_RESTRICT_LEGACY=1` |
| `Cargo.toml` | version `0.3.0` |

## Evidence

| Check | Result |
|-------|--------|
| `cargo test diagnostic` | 3 passed (missing config, no-path-leak, JSON keys) |
| `cargo test --test protocol_version` | 4 passed (absent/1/2 + GetProtocolVersion) |
| `cargo test --test restricted_methods` | 3 passed (default, opt-in, unaffected) |
| `cargo test service_state` | 5 passed |
| `cargo run -- diagnose` | five booleans + alias/address on this host |
| `cargo test` | 164 passed, 0 failed |
| `cargo test --test contract_fixtures` | 37/37 (additive method exempted, asserted fixture-free) |
| `cargo check --target i686-pc-windows-gnu` | 0 |

## Contract addition

`GetProtocolVersion` is the only new method. `tests/common::ADDITIVE_METHODS`
lists it; `every_expected_method_has_a_fixture` exempts it **and** asserts it has no
v0.2.0 fixture, so the frozen oracle cannot be silently widened. The addition is
documented in the fixture README and `RELEASE-NOTES.md`.

## Issue caught by the full suite

Adding the method to `EXPECTED_METHODS` broke TRANS-07's classification coverage
invariant. `GetProtocolVersion` is now classified `ReadOnly`. This is what the
suite is for.

## Self-Check: PASSED

- 164/0 tests; 37/37 contract; fmt/clippy/i686 clean; diagnose runs

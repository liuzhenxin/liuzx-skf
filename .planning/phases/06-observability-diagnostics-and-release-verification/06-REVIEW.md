---
phase: 06
phase_name: observability-diagnostics-and-release-verification
status: clean
depth: standard
findings_total: 4
high: 0
medium: 1
low: 3
fixed: 0
reviewed_at: 2026-09-12
reviewer: inline (no subagent runtime available)
---

# Phase 6 Code Review

Scope: `src/logging.rs`, `src/diagnostic.rs`, `src/service_state.rs`,
`src/protocol/mod.rs`, `src/domain/classification.rs`, `src/main.rs`,
`src/win_service.rs`, `packaging/windows/build.ps1`,
`.github/workflows/release-windows.yml`, and the new documentation.

## Findings

### M-1 (documented, environment-forced) — no external logger crate

CONTEXT D-01 selected an external logger. `cargo add flexi_logger --dry-run`
timed out because the configured registry is not reachable, so a new dependency
would break the build. The plan implements the same feature set in-repo. This is
recorded in `06-RESEARCH.md` §0, `06-01-SUMMARY.md`, and `RELEASE-NOTES.md`.

### L-1 (accepted) — `ts`, `updated`, `started_at` are Unix epoch seconds

Formatting UTC without a date crate would be hand-rolled, and the fields are
informational. Documented in `docs/WINDOWS-SERVICE.md`.

### L-2 (accepted) — the `diagnose` probe loads the vendor library

`library_loaded` genuinely calls `NativeSkfProvider::new`, which is the only way to
answer the question. It is a local, operator-invoked command and the load is
released immediately.

### L-3 (accepted) — a pre-existing debug `eprintln!` remains

`handle_request` prints `[DEBUG] Received method: ...` to stderr. It carries no
sensitive value (method name and parameter count) and the redaction scan would
flag a parameter dump. Removing it is cosmetic noise reduction deferred to a later
pass.

## Security notes

- `diagnose` output is an allowlist; a unit test asserts the resolved library path
  never appears in the JSON report.
- The redaction scan fails the build if a log macro names a PIN, key, session key,
  decrypted payload, certificate bytes, or request parameters.
- `apiVersion` is parsed with `#[serde(default)]`, so v0.2.0 clients are unaffected.
- `GetProtocolVersion` is an explicit additive method; the completeness test both
  exempts it and asserts it has no v0.2.0 fixture, so the oracle was not widened.
- Restricted methods are inert unless `SKF_RESTRICT_LEGACY=1`.
- The status file still contains no configuration values or secret-shaped fields.

## Verification

- `cargo test` → 164 passed, 0 failed
- `cargo test --test contract_fixtures` → 37/37
- `cargo fmt --all -- --check` → 0
- `cargo clippy --all-targets -- -D warnings` → 0
- `cargo check --target i686-pc-windows-gnu` → 0
- both workflow YAML files parse

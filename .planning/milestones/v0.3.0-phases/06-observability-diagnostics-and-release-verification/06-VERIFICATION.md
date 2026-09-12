---
phase: 06
slug: observability-diagnostics-and-release-verification
status: passed
verified_at: 2026-09-12
must_haves_total: 5
must_haves_verified: 5
must_haves_partial: 0
requirements_verified: [OBS-01, OBS-02, OBS-03, OBS-04, REL-01, REL-02, REL-03, REL-04, REL-05, DOC-01, DOC-02, DOC-03]
requirements_partial: []
human_verification_count: 4
verifier: inline (no subagent runtime available)
---

# Phase 6 — Verification

**Phase goal:** 让运维人员能够诊断问题而不泄露敏感信息，并让发布产物可被独立校验。

**Verdict:** `passed`. All twelve requirements have automated evidence; the frozen
contract is 37/37; and the release workflow/Windows lifecycle items that require a
runner are recorded for human verification.

## Automated Evidence

| Check | Command | Result |
|-------|---------|--------|
| Full suite | `cargo test` | 164 passed, 0 failed |
| Contract replay | `cargo test --test contract_fixtures` | 37/37 |
| Logging | `cargo test logging` | 4 passed |
| Redaction scan | `cargo test --test log_redaction` | 3 passed |
| Diagnostics | `cargo test diagnostic` | 3 passed |
| Protocol version | `cargo test --test protocol_version` | 4 passed |
| Restricted methods | `cargo test --test restricted_methods` | 3 passed |
| Formatting / lint / i686 | `cargo fmt --check` / `clippy -D warnings` / `check --target i686-pc-windows-gnu` | 0 / 0 / 0 |
| Workflows parse | `ruby -ryaml` on both workflow files | ok |
| Schema drift | `gsd-tools verify schema-drift 06` | `drift_detected: false` |

## ROADMAP Success Criteria

| # | Criterion | Status |
|---|-----------|--------|
| 1 | Logs are structured, leveled, rotating with bounded retention; proven to contain no PIN/key/payload | Met (JSON lines, 10 MiB × 7; redaction scan) |
| 2 | A local diagnostic reports the five stages using only non-sensitive facts | Met (`diagnose`; path-leak unit test) |
| 3 | v0.2.0 clients work unmodified; a version indicator exists; restricted methods return a documented error | Met (`apiVersion`, `GetProtocolVersion`, `SKF_RESTRICT_LEGACY`) |
| 4 | The ZIP ships a SHA-256 and the workflow verifies contents + `0x014C` | Met (build + workflow; runtime is a human item) |
| 5 | Threat model and Chinese/English docs describe boundary, session model, limits, restricted methods, layout | Met (`THREAT-MODEL.md`, `docs/SESSION-AND-LIMITS.md`, updated READMEs/agent docs) |

## Requirements

OBS-01..04, REL-01..05, DOC-01..03 are all `Complete` in `REQUIREMENTS.md`.

## Deviations

1. **No external logger crate** — registry unreachable; an in-repo rotating logger
   provides the same feature set (recorded in RESEARCH §0, the summary, and
   RELEASE-NOTES).
2. **`GetProtocolVersion` is an additive v0.3.0 method** — it has no v0.2.0 fixture
   and is listed in `ADDITIVE_METHODS`; the completeness test asserts it does not
   acquire one.
3. **Timestamps are epoch seconds**, not ISO-8601.

## Human Verification Required

`06-HUMAN-UAT.md` records: a live release-workflow run (checksum + extraction +
`0x014C`), rotation under a real long-running Windows service, `diagnose` on a real
machine, and the phase-5 Windows service lifecycle.

## Notes

- Adding `GetProtocolVersion` broke the TRANS-07 classification coverage invariant;
  the method was classified `ReadOnly`. The full suite caught it before completion.
- The verifier ran inline because this runtime exposes no `Task` subagent API.

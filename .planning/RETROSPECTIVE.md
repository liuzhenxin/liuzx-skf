# Project Retrospective

*A living document updated after each milestone. Lessons feed forward into future planning.*

## Milestone: v0.3.0 — Production Hardening

**Shipped:** 2026-09-12
**Phases:** 6 | **Plans:** 20 | **Sessions:** multiple (one per GSD phase)

### What Was Built
- v0.2.0 JSON-RPC contract frozen as 37 fixtures with a normalization oracle; the
  crate split into a library + thin binary so hardware-free tests could exist.
- Session-scoped authorization (deadline only, no retained PIN) and opaque
  session-owned handles, with deterministic release on disconnect and error paths.
- Bounded transport (1 MiB frame / 256 KiB payload / 64 connections / 30 s),
  whole-request blocking dispatch behind a per-provider FFI gate, and a
  loopback-only default.
- A real merge gate: rustfmt-clean, clippy-clean under `-D warnings`, pinned
  toolchain, six-job CI, and a gate self-test.
- An honest Windows service (staged startup, specific exit codes, stage-aware
  status, restrictive install ACL, upgrade/uninstall waits).
- Structured rotating logs, non-sensitive `diagnose`, protocol version, checksummed
  releases, threat model and bilingual docs.

### What Worked
- **The frozen fixture oracle.** Every refactor from phase 1 to 6 was judged by the
  same 37-fixture replay; it caught real drift (the `DisConnectDev` integer case,
  the empty-`symKey` case) that compiles and unit tests missed.
- **Adversarial/structural tests over grep.** The single-`unsafe` invariant, the
  classification coverage test, and the log-redaction source scan turned
  conventions into build failures.
- **Recording decisions before coding.** CONTEXT files repeatedly surfaced a
  constraint (serde flatten, `WaitForDevEvent` cancel, Linux CI without the vendor
  library) before it caused a wrong implementation.

### What Was Inefficient
- **Bookkeeping lag.** `phase complete` did not tick the ROADMAP phase checkboxes
  or update the Progress table; they were reconciled only at milestone close. The
  STATE.md plan counter also failed to advance due to a format drift.
- **Environment limits late.** The registry being unreachable was discovered at
  phase-6 planning, forcing the logger deviation; checking dependency availability
  earlier would have avoided the replan.
- **Nyquist frontmatter.** VALIDATION.md files for phases 3-6 stayed `draft`
  although their VERIFICATIONs passed, leaving an advisory gap.

### Patterns Established
- Contract additions are explicit: `ADDITIVE_METHODS` both exempts a new method and
  asserts it has no frozen fixture.
- Diagnostic output is an allowlist of non-sensitive fields, unit-tested against
  path leakage.
- Redaction is enforced by a source scan with a reasoned `// redaction-allow:`
  escape hatch.
- Environment-forced deviations are recorded in RESEARCH, the plan SUMMARY, and
  `RELEASE-NOTES.md`.

### Key Lessons
1. A compiled-but-clean tree is not evidence; read the diff hunk by hunk after any
   scripted edit, and let the behavioural oracle decide.
2. "Hardware-free" and "vendor-library-free" are different properties; CI job
   placement must account for the second.
3. Cross-phase invariants (classification coverage, single `unsafe`) are cheap to
   maintain and catch integration breakage the moment a new method is added.

### Cost Observations
- Model mix: single-model inline execution (no subagent runtime available).
- Sessions: one GSD command per phase.
- Notable: running agents inline kept context coherent but made each phase a long
  single session; subagents would have preserved more orchestrator context.

---

## Cross-Milestone Trends

### Process Evolution

| Milestone | Sessions | Phases | Key Change |
|-----------|----------|--------|------------|
| v0.3.0 | several | 6 | First GSD milestone: fixture-first refactor, wave execution, milestone audit |

### Cumulative Quality

| Milestone | Tests | Contract | Zero-Dep Additions |
|-----------|-------|----------|--------------------|
| v0.3.0 | 164 | 37/37 | 1 avoided (logger implemented in-repo) |

### Top Lessons (Verified Across Milestones)

1. Freeze the external contract before any structural refactor.
2. Turn every safety convention into a test that fails the build.

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

## Milestone: v0.4.0 — Secure Remote Operation

**Shipped:** 2026-09-13
**Phases:** 4 | **Plans:** 11

### What Was Built
- Optional TLS termination with the `ring` provider pinned so the i686 Windows
  cross-build stays reproducible; default stays plaintext loopback.
- mTLS (`WebPkiClientVerifier`) and bearer-token authentication enforced before a
  session exists; client identity threaded into the session.
- A single exposure rule: non-loopback requires opt-in + TLS + client auth.
- Structured, non-sensitive authorization audit emitted from the one decision
  point (`SessionState`).
- Docs, CI fast-job coverage, and the retroactive Nyquist sign-off for phases 3-6.

### What Worked
- **Attaching audit to the single chokepoint.** Every authorization decision
  already funnelled through three `SessionState` methods, so the audit was one
  change instead of dozens of call-site edits.
- **Pinning the crypto provider early.** Choosing `ring` explicitly (rather than
  the crate-feature default `aws-lc-rs`) kept the i686 cross-check green with no
  rework.
- **Hermetic certificates with `rcgen`.** TLS/auth/audit suites need no token or
  middleware, so they run in the hosted Linux CI job.
- **The frozen fixture oracle still paid off.** With the token re-attached, the
  37-fixture replay confirmed the default path was untouched by phase 7-9.

### What Was Inefficient
- The cargo registry mirror had to be switched to a sparse index before any new
  dependency could resolve; this blocked execution until fixed.
- The GM3000 token moved between the macOS host and the Windows VM mid-milestone,
  which temporarily blocked the contract replay and forced a carried blocker note.
- The audit end-to-end proof had to be library-level rather than a subprocess
  stderr capture, because without a token every handler fails before its
  authorization check.

### Patterns Established
- Security controls are opt-in and default-safe: no config means the old,
  already-verified behaviour.
- One decision point per concern (authorization; bind exposure) so cross-cutting
  behavior (audit, policy) has a single place to attach.
- A secret never gets a type that could serialize into a log; the audit event enum
  makes leaking one impossible by construction.

### Key Lessons
- Structural Nyquist sign-off drifts: a phase-3 grep check was superseded by a
  later phase. The audit should re-run commands, not trust the original text.
- Overlapping requirement scopes (AUTH-03 vs BIND-01) are fine if the boundary is
  written down; phase 8 enforced "none is loopback-only" and phase 9 added the TLS
  half.

### Cost Observations
- Four phases, eleven plans, executed inline in one session. Verification was
  dominated by the i686 cross-check (~80 s cold); the full test suite runs in
  seconds.

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

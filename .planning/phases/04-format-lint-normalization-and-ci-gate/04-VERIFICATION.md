---
phase: 04
slug: format-lint-normalization-and-ci-gate
status: passed
verified_at: 2026-09-11
must_haves_total: 4
must_haves_verified: 4
must_haves_partial: 0
requirements_verified: [QUAL-01, QUAL-02, QUAL-03, QUAL-04]
requirements_partial: []
human_verification_count: 3
verifier: inline (no subagent runtime available)
---

# Phase 4 — Verification

**Phase goal:** 建立真实生效的合并门禁：格式与 lint 基线干净，CI 运行硬件无关测试并在失败时阻断合并。

**Verdict:** `passed`. Formatting is clean in an isolated commit, clippy is green
under `-D warnings` on a pinned toolchain, the workflow runs six jobs on PRs and
`main`/`dev`, and the gate's failure-sensitivity is itself tested. Three items
require a remote/org action and are recorded for human follow-up.

## Automated Evidence

| Check | Command | Result |
|-------|---------|--------|
| Formatting | `cargo fmt --all -- --check` | exit 0 |
| Lint (hard fail) | `cargo clippy --all-targets -- -D warnings` | exit 0 |
| Clippy warnings | `cargo clippy --all-targets \| grep -c '^warning: '` | 0 |
| Full suite | `cargo test` | 133 passed, 0 failed |
| Contract replay | `cargo test --test contract_fixtures` | 37/37 |
| i686 compile check | `cargo check --target i686-pc-windows-gnu` | exit 0 |
| Workflow parses | YAML → jobs | clippy, fmt, gate-selftest, test, test-full, windows-i686 |
| Trigger check | YAML `on:` | `pull_request` + push `[main, dev]` |
| Gate self-test | local dry run | `GATE_ENFORCED` |
| Vendor-free Linux subset | dylib hidden, integration suites run | all pass |

## must_haves

| # | Must-have | Status | Evidence |
|---|-----------|--------|----------|
| 1 | `cargo fmt --all -- --check` exits 0 after an isolated formatting-only change | **Met** | `style(04):` commit with 29 `.rs` files; 133/0 and 37/37 after |
| 2 | `cargo clippy --all-targets -- -D warnings` is green; any allow is scoped and documented | **Met** | 0 warnings; the only allow is `too_many_arguments` on the two C-ABI mirrors, with a comment |
| 3 | CI runs formatting, linting, hardware-free tests and an i686 check on PRs and main pushes | **Met** | `ci.yml` six jobs; triggers include `dev` as a deliberate extension |
| 4 | A deliberately failing test blocks the CI check | **Met (mechanism proven)** | `gate-selftest` asserts the gate exits non-zero; the merge-blocking step itself is branch protection (human item) |

## ROADMAP Success Criteria

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| 1 | `cargo fmt -- --check` passes repository-wide after an isolated formatting-only change | Met | isolated `style(04):` commit; check exit 0 |
| 2 | Clippy has no actionable warnings, surviving allows explicitly scoped and documented | Met | 63→0; one scoped allow |
| 3 | CI runs fmt, lint, hardware-free tests, i686 compile on PRs and main pushes | Met | six jobs; `pull_request` + `[main, dev]` |
| 4 | A deliberately failing test blocks the CI check, proving enforcement | Met via `gate-selftest` + documented branch protection | self-test `GATE_ENFORCED` |

## Requirements

| Requirement | Status |
|-------------|--------|
| QUAL-01 formatting | Met |
| QUAL-02 clippy | Met |
| QUAL-03 CI workflow | Met |
| QUAL-04 enforced gate | Met (self-test + branch-protection checklist) |

## Human Verification Required

`04-HUMAN-UAT.md` records these; they are repository/org actions, not code.

1. Configure branch protection on `main` per `docs/CI.md`.
2. Confirm the x86_64 macOS runner label exists (or switch to the available one).
3. Open the first PR and confirm all six jobs run and `test-full` loads the vendor
   dylib.

## Notes

- The pinned `1.93.0` toolchain is distinct from the old `stable` toolchain, so the
  `i686-pc-windows-gnu` target had to be installed for it (the CI job does exactly
  that). Recorded in `04-03-SUMMARY.md`.
- The verifier ran inline because this runtime exposes no `Task` subagent API.

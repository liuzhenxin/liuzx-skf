---
phase: 04-format-lint-normalization-and-ci-gate
plan: 03
subsystem: ci
tags: [ci, github-actions, qual-03, qual-04]

requires: [04-01, 04-02]
provides:
  - .github/workflows/ci.yml with six jobs
  - docs/CI.md with the branch-protection checklist
  - an executable gate self-test
affects: [phase-5, phase-6]

tech-stack:
  added: []
  patterns:
    - "The full suite (incl. contract replay) runs on x86_64 macOS; Linux runs the vendor-free subset"
    - "The gate's failure-sensitivity is itself tested"

key-files:
  created:
    - .github/workflows/ci.yml
    - docs/CI.md

key-decisions:
  - "test-full pins x86_64 macOS (macos-13) because the committed vendor dylib is x86_64"
  - "Branch protection is documented, not configured, because repository files cannot set it"
  - "gate-selftest proves the check is failure-sensitive; it does not itself block merges"

requirements-completed: [QUAL-03, QUAL-04]

duration: 35min
completed: 2026-09-11
---

# Phase 4 Plan 3: CI Gate, Contract Replay Runner, and Gate Self-Test Summary

**A six-job CI workflow now runs on PRs and pushes to `main`/`dev`, keeps the contract replay on an x86_64 macOS runner, and includes a job that proves a failing test turns the gate red.**

## Jobs

| Job | Runner | Purpose |
|-----|--------|---------|
| `fmt` | ubuntu-latest | `cargo fmt --all -- --check` |
| `clippy` | ubuntu-latest | `cargo clippy --all-targets -- -D warnings` |
| `test` | ubuntu-latest | lib + invariant + vendor-free integration suites |
| `test-full` | x86_64 macOS (`macos-13`) | full `cargo test`, including the 37-fixture replay |
| `windows-i686` | ubuntu-latest | `cargo check --target i686-pc-windows-gnu` (installs mingw + target) |
| `gate-selftest` | ubuntu-latest | fails a test, asserts the gate exits non-zero |

Triggers: `pull_request` (all) and `push` to `[main, dev]`. `permissions: contents:
read`; superseded runs are cancelled via `concurrency`.

## Evidence

| Check | Result |
|-------|--------|
| YAML parses | 6 jobs: clippy, fmt, gate-selftest, test, test-full, windows-i686 |
| Trigger check | `pull_request` + `branches: [main, dev]` present |
| `clippy` uses `-D warnings` | yes |
| x86_64 macOS runner for the replay | `test-full` → `macos-13` |
| Self-test dry run | `GATE_ENFORCED`; `tests/ci_selftest.rs` created and removed, `git status` clean |
| Job commands 1–5, 7 locally | exit 0 |
| Full suite (`test-full` equivalent) | 133 passed, 0 failed; `contract_fixtures` 37/37 |

## Runner label

`test-full` uses `macos-13`. If that label is retired in the org, switch to the
available x86_64 macOS image (e.g. `macos-15-intel`); the requirement is an
**x86_64** runner so the committed x86_64 dylib loads. The first CI run is the
confirmation.

## Deviation / environment note

Pinning the toolchain to `1.93.0` created a distinct rustup toolchain that did not
have the `i686-pc-windows-gnu` target the old `stable` toolchain had, so the local
i686 check initially failed with `can't find crate for std`. Running
`rustup target add i686-pc-windows-gnu` (exactly what the CI job does) fixed it and
the check passes. This is expected: the target is installed per toolchain, which is
why the workflow installs it explicitly.

## Human follow-up (outside the repo)

- Configure branch protection on `main` per `docs/CI.md` (repository files cannot
  set it).
- Confirm on the first PR that all six jobs appear and `test-full` loads the
  vendor dylib.

## Self-Check: PASSED

- `grep -cE '^  (fmt|clippy|test|test-full|windows-i686|gate-selftest):' .github/workflows/ci.yml` → 6
- `docs/CI.md` contains the branch-protection checklist
- Local job commands all exit 0 (after installing the pinned-toolchain target)

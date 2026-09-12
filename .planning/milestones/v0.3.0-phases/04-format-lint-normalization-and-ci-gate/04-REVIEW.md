---
phase: 04
phase_name: format-lint-normalization-and-ci-gate
status: clean
depth: standard
findings_total: 3
high: 0
medium: 1
low: 2
fixed: 0
reviewed_at: 2026-09-11
reviewer: inline (no subagent runtime available)
---

# Phase 4 Code Review

Scope: the phase-4 changes — repository-wide formatting, the clippy fixes,
`rust-toolchain.toml`, `.github/workflows/ci.yml`, and `docs/CI.md`.

## Findings

### M-1 (documented, external) — the x86_64 macOS runner label is unverified

`test-full` pins `macos-13`. GitHub retires runner images over time and the label
cannot be validated from inside this repository. If it is unavailable, the job
will not start and the replay would have no home. `docs/CI.md` and
`04-03-SUMMARY.md` both record the fallback (an available x86_64 macOS image, e.g.
`macos-15-intel`) and the requirement (x86_64, because the committed vendor dylib
is x86_64). The first CI run is the confirmation.

### L-1 (accepted) — `gate-selftest` cannot distinguish "test failed" from "test failed to compile"

The job treats any non-zero `cargo test --test ci_selftest` as the gate working.
A compile failure of the synthetic file would also be non-zero. The synthetic file
is a two-line, dependency-free test, so this is a theoretical concern; the job
still proves that a red test command is not swallowed.

### L-2 (accepted) — CI jobs install the toolchain implicitly

The `fmt`/`clippy`/`test` jobs rely on the GitHub runner's rustup reading
`rust-toolchain.toml` and installing 1.93.0 with the declared components. That is
standard behaviour, but it means a broken `rust-toolchain.toml` would fail at the
first `cargo` step rather than at an explicit setup step. Adding `dtolnay/rust-toolchain`
was considered and rejected to keep third-party actions out of the gate.

## Security notes

- The workflow is `permissions: contents: read` and uses only `actions/checkout`.
- The gate blocks on a failing test by construction and that property is itself
  tested.
- No secret, token or vendor credential is referenced by the workflow.
- Moving the vendor dylib away made every vendor-free integration suite pass,
  which confirms the Linux `test` job does not silently depend on the vendor
  middleware (and therefore will not be a false red on Linux).

## Verification

- `cargo fmt --all -- --check` → 0
- `cargo clippy --all-targets -- -D warnings` → 0
- `cargo test` → 133 passed, 0 failed
- `cargo test --test contract_fixtures` → 37/37
- `cargo check --target i686-pc-windows-gnu` → 0
- YAML parses to six jobs with the intended triggers

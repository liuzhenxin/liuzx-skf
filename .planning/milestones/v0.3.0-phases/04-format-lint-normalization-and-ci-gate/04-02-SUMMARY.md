---
phase: 04-format-lint-normalization-and-ci-gate
plan: 02
subsystem: repo
tags: [clippy, toolchain, qual-02]

requires: [04-01]
provides:
  - rust-toolchain.toml pinning 1.93.0
  - clean cargo clippy --all-targets baseline (0 warnings)
affects: [04-03]

tech-stack:
  added: []
  patterns:
    - "Fix every actionable lint; the only allow is scoped to the C ABI-mirroring functions with a comment"

key-files:
  created:
    - rust-toolchain.toml
  modified:
    - src/main.rs
    - src/config/mod.rs
    - src/domain/container.rs
    - src/protocol/params.rs
    - src/skf/api.rs
    - tests/loopback_gate.rs
    - tests/transport_limits.rs

key-decisions:
  - "The two too_many_arguments warnings are allowed, not fixed: the functions mirror SKF_EncryptData/SKF_DecryptData"
  - "loopback_gate's env lock became a tokio mutex because a blocking guard was held across await"

requirements-completed: [QUAL-02]

duration: 40min
completed: 2026-09-11
---

# Phase 4 Plan 2: Clean Clippy Baseline and Toolchain Pin Summary

**`cargo clippy --all-targets -- -D warnings` is green (63 → 0 warnings) and the toolchain is pinned to 1.93.0; the only surviving allow is scoped to the two functions that mirror the SKF C ABI.**

## 63 → 0 disposition

| Lint | Count | Disposition |
|------|-------|-------------|
| `get_first` | 25 | fixed (`.first()`) |
| `needless_return` | 12 | fixed |
| `needless_borrows_for_generic_args` | 9 | fixed (`Command::args` arrays) |
| `await_holding_lock` | 4 | fixed: `loopback_gate` env lock is now `tokio::sync::Mutex` |
| `too_many_arguments` | 2 | **scoped allow** on `encrypt_data`/`decrypt_data` + comment (C ABI mirror) |
| `single_match` | 1 | fixed (`if let`) |
| `missing_safety_doc` | 1 | fixed: `get_func` gained a `# Safety` section |
| `for_kv_map` | 1 | fixed (`.keys()`) |
| `collapsible_str_replace` | 1 | fixed (`replace(['\n','\r'], "")`) |
| `bool_assert_comparison` | 1 | fixed (`assert!`) |

## Evidence

| Check | Result |
|-------|--------|
| `cargo clippy --all-targets -- -D warnings` | exit 0 |
| `cargo clippy --all-targets` warning count | 0 |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo test` | 133 passed, 0 failed |
| `cargo test --test contract_fixtures` | 37/37 |
| `rust-toolchain.toml` | `channel = "1.93.0"`, components rustfmt+clippy |
| `grep -c 'allow(clippy::too_many_arguments)' src/skf/api.rs` | 2 |

## Notes

- `cargo clippy --fix` applied the mechanical fixes; the resulting diff was read
  hunk-by-hunk. Every change is an equivalence (`get(0)`→`first()`, `&[...]`→`[...]`,
  `match`→`if let`, etc.).
- The `await_holding_lock` fix is the only substantive one: the old code held a
  `std::sync::MutexGuard` across `bind(...).await`, which can deadlock a
  single-threaded runtime. It is now an async mutex.
- The two ABI functions keep their signatures deliberately; refactoring them would
  break the one-to-one correspondence with the vendor API.

## Self-Check: PASSED

- 0 clippy warnings with `-D warnings`
- 133/0 tests, 37/37 contract
- toolchain pinned and resolvable (`rustup show` → 1.93.0-x86_64-apple-darwin)

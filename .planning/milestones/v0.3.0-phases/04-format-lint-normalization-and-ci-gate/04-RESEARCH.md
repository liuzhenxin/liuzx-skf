# Phase 4: Format/Lint Normalization and CI Gate — Research

**Researched:** 2026-09-11
**Method:** local measurement on this repository (rustc 1.93.0) plus inspection of
the existing workflow and test harness. No live web access; GitHub Actions runner
labels marked "verify at implementation time" are the only unverified items.

**Question answered:** "What do I need to know to PLAN this phase well?"

---

## 1. Formatting baseline (QUAL-01)

Measured:

```
cargo fmt --check   → 233 `Diff in` entries across multiple files, non-zero exit
```

There is **no** `rustfmt.toml` / `.rustfmt.toml`. All 233 diffs come from default
rustfmt behaviour (line wrapping, closure formatting, line-ending normalization).
The largest offenders are `src/main.rs`, `src/config/mod.rs`, and the phase-2/3
test modules.

**Implications for planning:**

- One `cargo fmt --all` runs the whole repo; there is no per-crate split.
- The change must be an isolated commit so `git blame`/review can ignore it.
- `rustfmt` never changes behaviour, but the proof is still `cargo test`
  (especially `contract_fixtures` 37/37).

## 2. Clippy baseline (QUAL-02)

Measured with `cargo clippy --all-targets`:

| Lint | Count | Notes |
|------|-------|-------|
| `clippy::get_first` | 25 | `req.params.get(0)` → `.first()`; concentrated in `src/main.rs` D-17 branches |
| `clippy::needless_return` | 12 | `return X` at block end |
| `clippy::needless_borrows_for_generic_args` | 9 | remove redundant `&` |
| `clippy::await_holding_lock` | 4 | `tests/loopback_gate.rs` holds a `std::sync::MutexGuard` across `.await` |
| `clippy::too_many_arguments` | 2 | `src/skf/api.rs::{encrypt_data,decrypt_data}` mirror the C ABI |
| `clippy::single_match` | 1 | `tests/transport_limits.rs` |
| `clippy::missing_safety_doc` | 1 | `src/skf/api.rs::get_func` |
| `clippy::for_kv_map` | 1 | prefer `.values()` |
| `clippy::collapsible_str_replace` | 1 | `src/domain/container.rs` |
| `clippy::bool_assert_comparison` | 1 | `src/protocol/params.rs` test |

**Two warnings are real, not cosmetic:**

- `await_holding_lock` is a genuine hazard (a `std` guard held across an await can
  deadlock a single-threaded runtime). Fix by using `tokio::sync::Mutex` in the
  async test.
- `missing_safety_doc` on an `unsafe fn` is a documentation gap.

`too_many_arguments` is the one case where the warning is wrong for this codebase:
the functions mirror `SKF_EncryptData` / `SKF_DecryptData`, so grouping arguments
would break the ABI correspondence. A scoped `#[allow]` with a comment is the
documented exception QUAL-02 allows.

**Toolchain:** `cargo --version` is 1.93.0 here, but there is no
`rust-toolchain.toml`. Clippy's lint set changes between releases, so an unpinned
CI would red-line on an image upgrade with no code change.

## 3. CI surface (QUAL-03)

Existing: only `.github/workflows/release-windows.yml` (dispatch + `v*` tags).
Default branch is `main`; the active integration branch is `dev`.

### 3.1 The contract replay is not "Linux-runnable"

`tests/contract_fixtures.rs` starts the real binary and asserts the recorded
v0.2.0 responses. Those fixtures were recorded against the macOS vendor middleware
`native/GM3000/macos/x86_64/libgm3000.1.0.dylib` (committed, 656 KB, x86_64).

- On an x86_64 macOS runner the dylib loads and all 37 fixtures match (this is how
  they pass locally).
- On a Linux runner the config resolves the macOS path to its Linux sibling
  (`native/GM3000/linux/libgm3000.1.0.so`), which is **not committed**. The
  provider factory falls back to `UnavailableProvider`, so every fixture that
  expects `ConnectDev failed: 0x0A000023` would instead see `Load Lib Failed` and
  drift.
- On an arm64 macOS runner, a freshly built arm64 test binary cannot load the
  x86_64 dylib.

**Conclusion:** the authoritative full suite (including the contract replay) must
run on an **x86_64 macOS runner**. GitHub labels to consider: `macos-13` (Intel,
historically available) and `macos-15-intel` (newer Intel image). The exact label
must be confirmed at implementation time; if neither is available, the fallback is
to cross-compile the test binary for `x86_64-apple-darwin` and run it under
Rosetta on an arm64 runner, or to keep the replay on a self-hosted x86_64 macOS.

### 3.2 Hardware-free tests

These do not touch the vendor library and can run on `ubuntu-latest`:
`cargo test --lib`, `--test provider_invariants`, `--test session_invariants`,
`--test classification_invariants`. They are the fast Linux signal.

`--test fixture_oracle` is pure normalization logic and also Linux-safe.

The remaining integration tests (`session_isolation`, `session_lifecycle`,
`transport_limits`, `ffi_serialization`, `loopback_gate`) start the binary but do
not depend on vendor responses; they are also Linux-safe and can run in the
`test` job. Keeping the replay out of the Linux job is the only necessary split.

### 3.3 i686 check

`cargo check --target i686-pc-windows-gnu` works on Linux after installing
`mingw-w64` (`sudo apt-get install -y mingw-w64`). The Rust target itself is
installed by `rustup target add i686-pc-windows-gnu`.

## 4. Gate enforcement (QUAL-04)

The workflow existing on PRs is necessary but not sufficient: "blocks merge"
requires GitHub branch protection, which is **not** expressible in a repository
file. What a repository *can* prove is that the check command fails on a failing
test. A `gate-selftest` job that writes a deliberately failing test, runs the same
`cargo test` command, and asserts a non-zero exit proves the check is
failure-sensitive; the branch-protection checklist documents the rest.

Dry-running the self-test locally:

```
printf '#[test]\nfn deliberate_failure() { assert!(false); }\n' > tests/ci_selftest.rs
cargo test --test ci_selftest ; test $? -ne 0   # must be true
rm tests/ci_selftest.rs
```

---

## Validation Architecture

This section feeds `04-VALIDATION.md`.

**Framework:** Rust built-ins (`cargo test`) plus `rustfmt`/`clippy`; workflow
behaviour is validated by running the same commands locally.

**Quick run:** `cargo fmt --all -- --check` (seconds)
**Lint run:** `cargo clippy --all-targets -- -D warnings`
**Full run:** `cargo test` (133 tests, 37-fixture replay)
**Feedback latency:** < 15 s for fmt/clippy; ~7 s for the test suite on this host.

| Seam | How it is verified |
|------|--------------------|
| Formatting clean | `cargo fmt --all -- --check` exits 0 |
| No logic change from formatting | `cargo test` still 133/0 and 37/37 |
| Clippy clean | `cargo clippy --all-targets -- -D warnings` exits 0 |
| Scoped allow documented | grep for the allow + adjacent comment in `src/skf/api.rs` |
| Toolchain pinned | `rust-toolchain.toml` contains `channel = "1.93.0"` and is parsed by `rustup show` |
| Workflow triggers | YAML contains `pull_request` and `push: branches: [main, dev]` |
| Workflow has all jobs | YAML contains `fmt`, `clippy`, `test`, `test-full`, `windows-i686`, `gate-selftest` |
| Contract replay on x86_64 macOS | `test-full` job uses an x86_64 macOS runner label |
| Gate is failure-sensitive | `gate-selftest` writes a failing test and asserts non-zero exit; dry-runnable locally |
| Branch protection documented | `docs/CI.md` lists the required repository settings |

**Manual-only:** whether GitHub branch protection is actually configured (a
repository setting, not a file), and whether the chosen x86_64 macOS runner label
is available in the org.

---

## Recommended task surface (input to planning)

- `04-01` formatting-only commit + proof.
- `04-02` toolchain pin + all 63 clippy fixes (mechanical first, then the two real
  ones, then the scoped ABI allow) + `-D warnings` proof.
- `04-03` `ci.yml` with six jobs + `docs/CI.md` + `gate-selftest` dry-run.

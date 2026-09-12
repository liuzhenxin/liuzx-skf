# Continuous Integration

The merge gate is `.github/workflows/ci.yml`. It runs on every pull request and on
pushes to `main` and `dev`.

## Jobs

| Job | Runner | Command | Why |
|-----|--------|---------|-----|
| `fmt` | ubuntu-latest | `cargo fmt --all -- --check` | Keeps the tree rustfmt-clean (QUAL-01) |
| `clippy` | ubuntu-latest | `cargo clippy --all-targets -- -D warnings` | Warnings are hard failures (QUAL-02) |
| `test` | ubuntu-latest | `cargo test --lib`, invariant suites, fixture structural checks, and the vendor-free integration suites | Fast hardware-free signal |
| `windows-i686` | ubuntu-latest | `cargo check --target i686-pc-windows-gnu` | Guards the PE32/i386 constraint |
| `gate-selftest` | ubuntu-latest | writes a failing test, asserts `cargo test` exits non-zero | Proves the gate is failure-sensitive (QUAL-04) |

## Local equivalents

Run the same commands before pushing:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo check --target i686-pc-windows-gnu
```

The i686 check needs the target once:

```bash
rustup target add i686-pc-windows-gnu
```

## The contract replay is a local/pre-release check, not a hosted job

`tests/contract_fixtures.rs` starts the real binary and replays the 37 frozen
v0.2.0 fixtures. That replay needs the GM3000 macOS **middleware** installed — not
just the committed dylib:

- On a runner that has the dylib but not the driver/middleware stack, the library
  loads and every `ConnectDev` returns `0x00000001`, whereas the fixtures recorded
  `0x0A000023` on a machine with the middleware. Confirmed on a hosted
  `macos-15-intel` runner (see the v0.3.0 CI run).
- A Linux runner resolves the config to `native/GM3000/linux/libgm3000.1.0.so`
  (not committed) and falls back to `UnavailableProvider`, which drifts for the
  same reason.

Widening `normalize` (or editing the fixtures) to absorb this would erase a real
assertion, so it is not an option.

**Before a release, run the replay on a machine with the vendor middleware:**

```bash
cargo test --test contract_fixtures   # 37/37 expected
```

Hosted CI still runs the fixture **structural** checks
(`fixture_count_is_at_least_37`, `every_expected_method_has_a_fixture`) and the
`fixture_oracle` self-test, so a dropped method or a broken oracle is caught there.
A self-hosted x86_64 macOS runner with the middleware could restore the full
replay as a hosted job.

## Branch protection

Branch protection is a repository setting, not a file. It is **configured** for
`main`: merges require these CI checks to pass (strict — the branch must be up to
date):

- `Formatting`
- `Clippy`
- `Hardware-free tests (Linux)`
- `Windows i686 compile check`
- `Gate self-test (a failing test must be caught)`

To re-apply or audit it:

```bash
gh api repos/:owner/:repo/branches/main/protection \
  --jq '.required_status_checks.contexts'
```

Without this setting, CI still runs but a red check does not block a merge.

## What `gate-selftest` proves — and what it does not

`gate-selftest` writes a deliberately failing test, runs the test command, and
asserts that the command exits non-zero. It therefore proves the **check command
is failure-sensitive**. It does not itself block a merge; that is the
branch-protection setting above.

Local reproduction:

```bash
printf '#[test]\nfn deliberate_failure() { assert!(false); }\n' > tests/ci_selftest.rs
if cargo test --test ci_selftest; then echo GATE_BROKEN; else echo GATE_ENFORCED; fi
rm -f tests/ci_selftest.rs
```

## Toolchain

`rust-toolchain.toml` pins Rust 1.93.0 with the `rustfmt` and `clippy` components,
so local runs and CI resolve the same lint baseline. GitHub runners install it
automatically via `rustup`.

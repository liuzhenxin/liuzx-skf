# Continuous Integration

The merge gate is `.github/workflows/ci.yml`. It runs on every pull request and on
pushes to `main` and `dev`.

## Jobs

| Job | Runner | Command | Why |
|-----|--------|---------|-----|
| `fmt` | ubuntu-latest | `cargo fmt --all -- --check` | Keeps the tree rustfmt-clean (QUAL-01) |
| `clippy` | ubuntu-latest | `cargo clippy --all-targets -- -D warnings` | Warnings are hard failures (QUAL-02) |
| `test` | ubuntu-latest | `cargo test --lib` and the invariant/no-vendor integration suites | Fast hardware-free signal |
| `test-full` | x86_64 macOS (`macos-15-intel`) | `cargo test` | The authoritative suite, including the frozen contract replay |
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

## Why the contract replay needs x86_64 macOS

`tests/contract_fixtures.rs` starts the real binary and replays the 37 frozen
v0.2.0 fixtures. The binary loads the vendor middleware
`native/GM3000/macos/x86_64/libgm3000.1.0.dylib` (committed, x86_64).

- A Linux runner resolves the config to `native/GM3000/linux/libgm3000.1.0.so`,
  which is not committed, so the provider falls back to `UnavailableProvider` and
  every fixture that expects `ConnectDev failed: 0x0A000023` drifts.
- An arm64 macOS runner cannot load the x86_64 dylib into a freshly built arm64
  test binary.

Therefore `test-full` pins an x86_64 macOS image (`macos-15-intel`; `macos-13`
was retired by GitHub). If that label is unavailable, pick another x86_64 macOS
label.

## Branch protection (required, set in GitHub, not in this repository)

Repository files cannot configure branch protection. To make the gate actually
block a merge, an administrator must:

1. Open **Settings → Branches** and add a branch protection rule for `main`.
2. Enable **Require status checks to pass before merging**.
3. Select the CI jobs: `fmt`, `clippy`, `test`, `test-full`, `windows-i686`,
   `gate-selftest`.
4. Optionally enable **Do not allow bypassing the above settings**.

Without step 2–3, CI still runs but a red check does not block a merge.

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

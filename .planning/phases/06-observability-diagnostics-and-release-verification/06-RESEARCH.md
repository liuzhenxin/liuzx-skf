# Phase 6: Observability, Diagnostics, and Release Verification — Research

**Researched:** 2026-09-11
**Method:** source-level analysis of this repository plus a live dependency check.
No Windows host; Windows-only pieces are compile-checked and human-verified.

**Question answered:** "What do I need to know to PLAN this phase well?"

---

## 0. Critical environment finding — no new dependencies

`cargo add flexi_logger --dry-run` **timed out** (the configured registry is the
Tsinghua mirror, `crates-io` is replaced by a non-remote `tuna` source, and the
index update did not complete). Adding any crate that is not already in
`Cargo.lock` is therefore unsafe for this phase.

**Consequence:** CONTEXT D-01 chose `flexi_logger`; the plan must instead
implement rotation + JSON-lines in-repo using `log` + `serde_json` (both already
dependencies). This is a forced, documented deviation. The feature set
(structured, leveled, size-rotated, bounded retention) is unchanged; only the
mechanism differs. A `log::Log` implementation is roughly 100 lines and is fully
unit-testable with a temp directory.

The same constraint applies to every other phase-6 task: diagnostics, protocol
versioning, restricted-method gating and release verification all use existing
dependencies only.

## 1. Logging today

- `src/main.rs:153` calls `env_logger::init()`.
- `src/win_service.rs:230` calls `env_logger::Builder::from_env(...).init()`.
- `src/win_service.rs::redirect_stdio_to_log` sends stdout/stderr to a single
  `skf-service.log` next to the exe. It is unbounded and not structured.
- `log::*` calls exist across `server`, `domain`, `session`, `win_service`; none
  interpolates a PIN, key, or request payload. A source-scan test will keep it
  that way.

**Design for the in-repo logger (`src/logging.rs`):**

```rust
pub const MAX_LOG_BYTES: u64 = 10 * 1024 * 1024; // 10 MiB
pub const KEEP_LOG_FILES: usize = 7;

pub fn init_console();                         // env_logger, stderr, RUST_LOG
pub fn init_service_file(dir: &Path);          // installs the rotating logger
pub fn sanitize(value: &str) -> String;        // strip control chars, truncate 512
```

- `RotatingFileLogger` implements `log::Log`; holds `Mutex<Inner>` with the open
  file, path, bytes written, and a `max_level` parsed from `RUST_LOG`.
- Each record is one JSON line: `{"ts":<epoch secs>,"level":"...","target":"...","message":"..."}`.
- Before writing, if `written + line.len() + 1 > MAX_LOG_BYTES`, rotate:
  `skf-service.log` → `.1`, `.1` → `.2`, … dropping files beyond `KEEP_LOG_FILES`.
- Rotation and retention are pure filesystem logic, testable on any platform.

## 2. Diagnostics

There is no diagnostic command. The building blocks already exist:

- `config::load` + `config::resolve_lib_path` answer "config loaded" and
  "provider resolved for this OS".
- `std::path::Path::new(&resolved).exists()` answers "library file present".
- Loading the library is exactly what `NativeSkfProvider::new` does; a diagnostic
  can attempt it and report success/failure as a **class** (not the raw path,
  addressing Phase 2 L-05).
- `server::bind_with_progress` (Phase 5) reports `config → provider → bind`; the
  diagnostic can reuse the lower-level calls or attempt a probe bind.

`service_state.rs` adds `started_at: Option<String>` (epoch seconds) so the
diagnostic can compute `uptime_seconds` while the service runs.

CLI wiring: `src/main.rs` dispatches `install`/`uninstall`/… under
`#[cfg(windows)]`. `diagnose` must run on **all** platforms, so it is handled
before that block.

## 3. Protocol version and restricted methods

`RpcRequest` (`src/protocol/mod.rs`) is:

```rust
pub struct RpcRequest { pub method: String, #[serde(default)] pub params: Vec<Value>, pub id: Option<Value> }
```

- Add `#[serde(default, rename = "apiVersion")] pub api_version: Option<u32>`.
  A v0.2.0 client omits it; serde ignores unknown fields, so old clients are
  unaffected.
- Version check happens in `handle_request` before the method match, so every
  method is covered and the rejection carries the request id.

`GetProtocolVersion` is a **new** method:

- `tests/common::EXPECTED_METHODS` drives fixture completeness
  (`every_expected_method_has_a_fixture`). A new method cannot have a v0.2.0
  fixture. Add an `ADDITIVE_METHODS` allowlist so the completeness test exempts
  it, and document the addition in the fixture README and `RELEASE-NOTES.md`.
- `service` is read from `env!("CARGO_PKG_VERSION")`, so the `Cargo.toml` bump to
  `0.3.0` is required.

Restricted methods:

- The set is `{IssueCertificate, Transmit}`. Default behaviour must not change
  (the `IssueCertificate` fixture and the `Transmit` fixture must still pass).
- Gate: when `SKF_RESTRICT_LEGACY=1`, these return `-100` with a documented
  message. A pure function `restricted_rejection(method) -> Option<RpcResponse>`
  is unit-testable without the env var by injecting the boolean.

## 4. Release integrity

`packaging/windows/build.ps1` already has `Get-PeMachine` and asserts the exe and
DLL are `0x014C`. `.github/workflows/release-windows.yml` builds, checks the ZIP
exists, uploads one artifact and publishes it on a tag.

Additions:

- Write `dist/<zip>.sha256` in `sha256sum` format (`<hex> *<name>`).
- Workflow: verify the hash with `Get-FileHash -Algorithm SHA256`; expand the ZIP
  into a temp dir; assert `skf-service.exe`, `mtoken_gm3000.dll`,
  `config\skf.yaml` and `api\` exist; assert both PE machines are `0x014C`;
  upload the `.sha256` alongside the ZIP and publish both on a tag.

## 5. Documentation inventory

- Existing: `README_CN.md`, `README_EN.md`, `AGENTS.md`, `CLAUDE.md`, `docs/CI.md`,
  `docs/OPERATION-CLASSIFICATION.md`, `docs/WINDOWS-SERVICE.md`.
- Missing: a threat model, a session/limits doc, release notes, and an update of
  the READMEs/agent docs to the post-refactor layout.

---

## Validation Architecture

This section feeds `06-VALIDATION.md`.

**Framework:** Rust built-ins; PowerShell for the release workflow (human/CI).
**Quick run:** `cargo test --lib`
**Full run:** `cargo test` + `cargo fmt --check` + `cargo clippy -D warnings` + i686
check.
**Latency:** < 25 s.

| Seam | Verified by |
|------|-------------|
| JSON-line format | unit test: a captured line parses and has the four fields |
| Rotation | unit test: writing > MAX_LOG_BYTES produces `.1` and keeps ≤ KEEP+1 files |
| Retention | unit test: many rotations leave at most KEEP files |
| `sanitize` | unit test: control chars removed, long input truncated |
| Redaction policy | `tests/log_redaction.rs` scans source for sensitive identifiers in log macros |
| `apiVersion` absent/present/too-new | integration: absent works; `apiVersion: 1` works; `apiVersion: 2` → `-1` |
| `GetProtocolVersion` | integration: returns `{min,current,service}`; contract addition recorded |
| Restricted default | integration: `IssueCertificate`/`Transmit` still behave as before |
| Restricted gate | unit: `restricted_rejection` returns `Some(-100)` only when enabled |
| diagnose | integration/library: five booleans present; `--json` parses; no path/secret |
| checksum | CI: `Get-FileHash` matches `.sha256` |
| extracted package | CI: required files present; exe/DLL `0x014C` |
| docs | structural: required files exist and contain the required sections |

**Manual-only:** the Windows release workflow itself (tag), and running
`verify-service.ps1` (Phase 5 item).

---

## Recommended task surface (input to planning)

- `06-01` in-repo rotating JSON logger + redaction scan (OBS-01/02).
- `06-02` `diagnose` + `apiVersion`/`GetProtocolVersion` + restricted gate + the
  `started_at` field (OBS-03/04, REL-01/02).
- `06-03` checksum + workflow verification + `RELEASE-NOTES.md` + threat model +
  bilingual session/limits doc + README/agent-doc updates (REL-03/04/05,
  DOC-01/02/03).

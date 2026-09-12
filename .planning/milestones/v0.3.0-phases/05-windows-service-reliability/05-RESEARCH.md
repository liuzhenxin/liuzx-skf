# Phase 5: Windows Service Reliability — Research

**Researched:** 2026-09-11
**Method:** source-level analysis of this repository plus the vendored
`windows-service-0.7.0` crate in the local cargo registry. No Windows host is
available, so runtime behaviour is planned for human UAT; compile-time and
pure-logic behaviour is verified on macOS/Linux.

**Question answered:** "What do I need to know to PLAN this phase well?"

---

## 1. Current defect surface

`src/win_service.rs::run_service`:

1. Reports `ServiceState::Running` **before** `server::run_server` (which loads
   config, resolves the provider and binds the listener). A bind failure therefore
   shows a briefly-Running then Stopped service.
2. On failure it reports `Stopped` with `ServiceExitCode::Win32(0)` — a clean
   exit — so `sc failure` restart actions do not fire.
3. `status` prints SCM state + pid + executable only; it cannot distinguish
   "process running" from "service usable".

`packaging/windows/install.ps1`:

- Sets restart actions via `sc failure ... restart/5000/restart/10000/restart/60000`.
- Copies payload to `%ProgramFiles(x86)%\LiuZX\SKF Service` but never sets an ACL
  explicitly.
- Replaces files after a fixed `Start-Sleep -Milliseconds 800`, with no check that
  the process exited or released the executable — the upgrade risk for SVC-04.

## 2. The `windows-service` crate capabilities (verified in vendored source)

- `ServiceStatus { service_type, current_state, controls_accepted, exit_code,
  checkpoint, wait_hint, process_id }` — supports `StartPending` with a
  monotonically increasing `checkpoint` and a `wait_hint`.
- `ServiceExitCode::{Win32, ServiceSpecific}` — a distinct service-specific code is
  representable and is what SVC-02 asks for.
- `service_control_handler::register` + `set_service_status` are already used.
- `ServiceFailureActions` and `update_failure_actions` exist, so restart actions
  *could* be configured from Rust; the current design keeps `sc failure` in the
  installer, which is sufficient and is left alone (D-07).
- The SCM requires the handler to respond to `Interrogate`; already handled.

## 3. Startup seam in `src/server/mod.rs`

`run_server` already carries `bound_tx: Option<oneshot::Sender<BoundServer>>` and
sends the resolved address immediately after `bind` succeeds. That is the "listener
is bound" signal SVC-01 needs.

However, `bind` internally performs three steps — `config::load`, provider build
(`NativeProviderFactory`), and `TcpListener::bind` — with no per-step visibility.
For SVC-05's stage reporting and SVC-02's distinct exit codes, the cleanest change
is a progress-aware variant:

```rust
pub enum StartupStage { Config, Provider, Bind }

pub async fn bind_with_progress<F>(
    opts: &ServerOptions,
    factory: &F,
    on_stage: impl Fn(StartupStage),
) -> Result<(BoundServer, TcpListener, PreparedServer), StartupError>;

pub async fn bind<F>(opts, factory) -> anyhow::Result<...>  // delegates with a no-op callback
```

with

```rust
pub enum StartupError {
    Config(String),       // load/parse failure            -> exit 1
    Provider(String),     // default alias/OS path missing -> exit 2
    Bind(String),         // socket bind failure           -> exit 3
}
```

`run_service` then calls `bind_with_progress` + `serve` itself (both are public),
writes the stage file, reports `Running` after bind, and maps `StartupError` to the
exit code. The working-directory fallback currently inside `run_server` should be
extracted so `run_service` can reuse it.

## 4. D-08 / D-09 boundary (critical, preserves Linux CI)

`NativeProviderFactory::build` never errors: a load failure returns
`UnavailableProvider`, which keeps the process answering with `Load Lib Failed`.
The Linux CI (`tests/session_isolation.rs`, `tests/transport_limits.rs`,
`tests/ffi_serialization.rs`) deliberately runs with **no vendor library** and
depends on the service starting.

Therefore:

- **Fatal** (non-zero exit): YAML unparseable; `default` alias not in `libs`; the
  default alias has **no path for the current OS** (`resolve_lib_path` fails).
- **Non-fatal** (Running, degrade to `UnavailableProvider`): the path is configured
  but the library file is missing / fails to load.

On Linux, `config/skf.yaml` defines a `linux:` path for GM3000, so
`resolve_lib_path` succeeds and the service still starts. This is exactly the
behaviour the CI relies on.

## 5. Status file design (cross-platform-testable)

A new module compiled on every platform (so macOS/Linux CI can unit-test it):

```rust
pub enum ServiceState { StartPending, Running, Failed, Stopped }
pub struct ServiceStatusFile {
    pub state: ServiceState,
    pub stage: &'static str,   // "config" | "provider" | "bind" | "serve"
    pub code: Option<u32>,
    pub reason: Option<String>,
    pub pid: Option<u32>,
    pub address: Option<String>,
    pub updated: Option<String>,
}
```

- Serialize with `serde_json` (already a dependency); no new crate.
- Atomic write: write `<file>.tmp` then `std::fs::rename` over the target.
- Exit-code constants (`EXIT_CONFIG = 1`, `EXIT_PROVIDER = 2`, `EXIT_BIND = 3`,
  `EXIT_SERVE = 4`) live here so the Rust writer, tests, and docs share one source.
- Never serialize config values, library paths, PINs, or key material.

`win_service.rs` writes it; `status` reads and prints it, falling back to SCM-only
output when absent (backward compatible).

## 6. Installer concerns

- **ACL:** `icacls "$dir" /inheritance:r /grant:r "SYSTEM:(OI)(CI)F"
  "Administrators:(OI)(CI)F" "Users:(OI)(CI)RX"`, then verify the `icacls` output
  does not grant `Users` write/modify/full. `$env:ProgramFiles(x86)` is the install
  root, and the service is LocalSystem, so it can still write its state file.
- **Wait instead of sleep:** after stopping, poll `sc query`/`Get-Service` for
  `Stopped` and attempt an exclusive open of the exe
  (`[IO.File]::Open($Exe,'Open','ReadWrite','None')`) until it succeeds or a 30 s
  timeout elapses.
- **Uninstall:** wait for process exit *and* registration removal before deleting
  the directory; a failed `Remove-Item` must not be swallowed.
- **Verification script:** `packaging/windows/verify-service.ps1` runs
  install → status (assert `Running`/`serve`) → install-over-running (upgrade) →
  status → uninstall (assert unregistered + directory removed) → reinstall →
  uninstall. This is the Windows human-UAT artefact.

## 7. Verification strategy on a macOS host

| Layer | What it proves | How |
|-------|----------------|-----|
| Cross-compile | Windows code still builds | `cargo check --target i686-pc-windows-gnu` and `x86_64-pc-windows-gnu` |
| Pure logic | state file round-trip, atomic write, exit-code mapping, reason redaction | `cargo test --lib` (new module tests, run on macOS) |
| Contract | no request-path drift | `cargo test --test contract_fixtures` 37/37 |
| Windows runtime | SCM ordering, real restart, ACL, upgrade locks | `verify-service.ps1` on a Windows VM (`05-HUMAN-UAT.md`) |

`cargo check` is mandatory in CI for the Windows target; the runtime script cannot
run in hosted CI without a Windows service-capable job, which is deferred.

---

## Validation Architecture

This section feeds `05-VALIDATION.md`.

**Framework:** Rust built-ins plus PowerShell for the Windows runtime script.
**Quick run:** `cargo test --lib` (state-file logic).
**Full run:** `cargo test` + `cargo check --target i686-pc-windows-gnu`.
**Feedback latency:** < 20 s locally.

| Seam | Verified by |
|------|-------------|
| State file serialize/parse/round-trip | unit tests in the new module |
| Atomic write (tmp + rename) | unit test: target absent until write completes; content parses |
| Reason/message redaction | unit test: serialized JSON contains no configured path or secret-shaped field |
| StartupStage → exit code | unit test over the mapping |
| `bind_with_progress` emits config→provider→bind | unit test with a test factory (no vendor library) and an ephemeral port |
| Fatal vs non-fatal provider boundary | unit test: missing OS path → `StartupError::Provider`; configured-but-missing file → `Ok` (degraded) |
| `status` prints stage | structural + Windows UAT |
| ACL applied and verified | `verify-service.ps1` on Windows |
| Upgrade/uninstall leave no residue | `verify-service.ps1` on Windows |
| Contract unchanged | `cargo test --test contract_fixtures` 37/37 |

**Manual-only:** everything SCM-runtime (StartPending/Running ordering, real restart
action firing, ACL enforcement, file locks) — Windows VM.

---

## Recommended task surface (input to planning)

- `05-01` cross-platform `service_state` module + `bind_with_progress`/`StartupError`
  + `run_service` rewrite + `status` stage output + unit tests.
- `05-02` installer ACL + robust wait + uninstall strictness + `verify-service.ps1`
  + docs.

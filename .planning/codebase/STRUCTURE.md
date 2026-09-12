# Codebase Structure

**Analysis Date:** 2026-09-11

## Directory Layout

```text
liuzx-skf/
├── src/                         # Rust service implementation
│   ├── main.rs                  # Hosting, JSON dispatch, workflows, DER helpers
│   ├── win_service.rs           # Windows SCM integration
│   └── skf/                     # SKF C ABI adapter
│       ├── mod.rs
│       ├── api.rs
│       └── types.rs
├── api/                         # Browser demo and JavaScript client
├── tests/                       # Hardware-backed Node/Bash integration tests
├── config/                      # Runtime provider configuration
├── native/                      # Committed vendor shared libraries
│   ├── GM3000/
│   └── FishMan/
├── packaging/windows/           # Windows build/install/run/uninstall assets
├── .github/workflows/           # Tag-driven release automation
├── .cargo/                      # Cross-compilation linker configuration
├── docs/                        # Promotional/support documentation and media
├── Cargo.toml                   # Rust manifest
├── package.json                 # Node test/client dependency manifest
└── README*.md                   # User-facing documentation
```

## Directory Purposes

**`src/`:**
- Purpose: All executable Rust code.
- Key files: `src/main.rs` and `src/win_service.rs`.
- Current shape: `src/main.rs` is approximately 3,680 lines and contains nearly all protocol and workflow logic.

**`src/skf/`:**
- Purpose: Vendor-neutral SKF foreign-function interface.
- `src/skf/types.rs`: C-compatible aliases, structures, handles, result codes, and algorithm constants.
- `src/skf/api.rs`: Dynamic symbol signatures and safe-looking wrapper methods around unsafe calls.
- `src/skf/mod.rs`: Declares the `api` and `types` modules.

**`api/`:**
- Purpose: Static demo UI and reusable JavaScript WebSocket client.
- Key files: `api/skf_api.html`, `api/skf_api.js`.
- Served directly by Warp; no compilation or bundling step.

**`tests/`:**
- Purpose: End-to-end checks against a running service and physical USB Key.
- Key files: `tests/test_all_apis.js`, `tests/test_usb_event.js`, `tests/run_tests.sh`.
- No Rust unit-test tree or fixture directory exists.

**`config/`:**
- Purpose: Runtime provider and OS-specific native-library paths.
- Key file: `config/skf.yaml`.

**`native/`:**
- Purpose: Selected vendor user-mode libraries distributed or used locally.
- Subdirectories: `native/GM3000/{windows,macos}` and `native/FishMan/windows`.
- Treat every binary as third-party, architecture-specific, and security-sensitive.

**`packaging/windows/`:**
- Purpose: Reproducible Windows x64-host/i686-process release assembly.
- Key files: `build.ps1`, `install.ps1`, `uninstall.ps1`, `run.bat`, and package-specific YAML/readme.
- Output is generated under ignored `dist/`.

**`.github/workflows/`:**
- Purpose: CI release packaging.
- Key file: `.github/workflows/release-windows.yml`.
- Current workflow builds on manual dispatch and on pushed `v*` tags.

**`.planning/`:**
- Purpose: GSD project context, codebase map, requirements, roadmap, and state.
- Committed: Yes, once initialized.

## Key File Locations

**Entry Points:**
- `src/main.rs` — executable and WebSocket/HTTP server entry.
- `src/win_service.rs` — Windows service dispatcher and management commands.
- `api/skf_api.js` — external JavaScript client entry.
- `api/skf_api.html` — browser demonstration entry.

**Configuration:**
- `Cargo.toml` and `Cargo.lock` — Rust package and resolved dependencies.
- `package.json` and `package-lock.json` — Node dependency metadata.
- `config/skf.yaml` — default multi-provider runtime config.
- `packaging/windows/skf-windows-x86.yaml` — Windows GM3000 package config.
- `.cargo/config.toml` — MinGW linker names.
- `.gitignore` — build, local state, logs, secrets, and accidental duplicate exclusions.

**Core Logic:**
- `src/main.rs:38` — `SkfContext` provider/path/API cache operations.
- `src/main.rs:238` — shared foreground/service server bootstrap.
- `src/main.rs:541` — request dispatcher and application workflows.
- `src/skf/api.rs` — native SKF method wrappers.
- `src/skf/types.rs` — ABI data model.

**Testing:**
- `tests/test_all_apis.js` — consolidated hardware workflow tests.
- `tests/test_usb_event.js` — continuous insertion/removal observer.
- `tests/run_tests.sh` — macOS-oriented build/deploy/test orchestration.

**Documentation:**
- `README_CN.md`, `README_EN.md`, `README.md` — user documentation.
- `packaging/windows/README-Windows-x86_64.md` — release/install/troubleshooting guide.
- `RTK.md` — runtime toolkit notes.
- `AGENTS.md`, `CLAUDE.md`, `GEMINI.md` — coding-agent guidance; parts currently lag implementation.

## Naming Conventions

**Rust Files and Modules:**
- snake_case: `win_service.rs`, `skf/mod.rs`.
- Functions and variables use snake_case: `run_server`, `get_lib_path`, `hash_handles`.
- Rust types use PascalCase except C ABI definitions, which intentionally preserve names such as `ECCPUBLICKEYBLOB`.
- Constants use UPPER_SNAKE_CASE: `SAR_OK`, `SGD_SM4_CBC`, `SERVICE_NAME`.

**JavaScript:**
- Files use snake_case: `skf_api.js`, `test_all_apis.js`.
- Class uses PascalCase (`SKFClient`); methods and variables use camelCase.

**Packaging:**
- Windows scripts use conventional action names: `build`, `install`, `run`, `uninstall`.
- Release folder names encode host OS and native/process architecture.

## Where to Add New Code

**New JSON-RPC Method:**
- Today: add dispatch logic to `src/main.rs`, add required SKF wrapper to `src/skf/api.rs`, update types/constants in `src/skf/types.rs`, expose client method in `api/skf_api.js`, and add a case in `tests/test_all_apis.js`.
- Preferred after refactoring: place typed request/handler code in dedicated protocol/domain modules rather than extending the monolithic match.

**New SKF Provider:**
- Add OS paths and optional VID:PID mapping in `config/skf.yaml`.
- Place approved user-mode libraries under `native/<Provider>/<platform>/` only when redistribution is permitted.
- Add provider-specific package config/scripts under `packaging/` and validate binary architecture.

**New Unit-Testable Logic:**
- Extract pure logic from `src/main.rs` into a library module under `src/`.
- Add `#[cfg(test)]` modules near implementation or integration tests under `tests/*.rs`.

**New Release Target:**
- Add platform-specific assembly under `packaging/<platform>/`.
- Add or extend workflows under `.github/workflows/`.
- Keep generated binaries in ignored `dist/` and `target/`.

**Documentation Changes:**
- Update both `README_CN.md` and `README_EN.md` for user-visible behavior.
- Update `AGENTS.md`/`CLAUDE.md` when architecture or commands change.

## Special Directories

**`target/`:**
- Cargo build output.
- Generated and ignored.

**`dist/`:**
- Assembled release directories and ZIP files.
- Generated and ignored.

**`logs/`:**
- Runtime logs and PID files.
- Generated and ignored.

**`node_modules/`:**
- npm-installed test/client dependencies.
- Generated and ignored.

**`native/`:**
- Third-party binary inputs, not generated.
- Selected files are committed; licensing and architecture must be reviewed before additions.

**`test_rcgen/`:**
- Local experimental nested repository/build area.
- Ignored and not part of the product architecture.

---

*Structure analysis: 2026-09-11*
*Update when source modules or packaging targets are reorganized*

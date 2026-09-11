# Coding Conventions

**Analysis Date:** 2026-09-11

## Naming Patterns

**Files:**
- Rust modules use snake_case: `src/win_service.rs`, `src/skf/api.rs`.
- JavaScript files use snake_case: `api/skf_api.js`, `tests/test_all_apis.js`.
- User-facing landmark documentation uses uppercase names: `README_CN.md`, `RTK.md`, `AGENTS.md`.

**Functions:**
- Rust functions use snake_case; async functions have no prefix (`run_server`, `handle_request`).
- SKF wrapper names translate C names to snake_case (`SKF_EnumDev` → `enum_dev`).
- JavaScript public client methods use camelCase (`enumProvider`, `createPKCS10`).

**Variables:**
- Rust local variables and fields use snake_case.
- Constants use UPPER_SNAKE_CASE.
- Intentionally unused values use a leading underscore, as in `_provider`.

**Types:**
- Native Rust types use PascalCase (`SkfConfig`, `SkfContext`, `RpcRequest`).
- C ABI aliases and structures retain specification names (`ULONG`, `DEVINFO`, `ECCPUBLICKEYBLOB`) and are protected by allow attributes in `src/skf/`.
- C-layout structures use `#[repr(C)]`; opaque handles are raw pointers.

## Code Style

**Formatting:**
- Intended formatter is rustfmt (`cargo fmt`).
- Current baseline is not rustfmt-clean: `cargo fmt -- --check` reports 198 diff sections, primarily in `src/main.rs` and `src/skf/api.rs`.
- New Rust modules should be rustfmt-formatted even before a dedicated whole-file normalization change.
- JavaScript uses four-space indentation, semicolons, single-quoted module imports, and template literals for output.
- PowerShell uses four-space indentation and PascalCase command conventions.

**Linting:**
- Clippy is the Rust linter (`cargo clippy --all-targets`).
- Current baseline succeeds but emits approximately 72 warnings per binary target, dominated by `req.params.get(0)`, needless borrows, acronym naming, and large function signatures.
- There is no configured JavaScript linter or formatter and no npm scripts.

## Import Organization

**Rust:**
1. Module declarations and platform-gated modules.
2. Standard library imports.
3. External crates.
4. Internal crate imports.
- Existing `src/main.rs` does not consistently group or alphabetize imports; use rustfmt-compatible grouping in new modules.

**JavaScript:**
1. Local client module.
2. External `ws` dependency.
3. Node built-ins.
- CommonJS `require()` is the established module format.

## Error Handling

**Startup and CLI:**
- Return `anyhow::Result<()>` and add context with `anyhow!` or `.context()`.
- Fail startup when config parsing or address binding fails.

**Request Boundary:**
- Use early returns when positional parameters are missing or invalid.
- Return `RpcResponse::err` with numeric code, localized message where implemented, and original request ID.
- Return `RpcResponse::ok` with JSON values for success.

**Native Calls:**
- Compare every SKF return value with `SAR_OK`.
- Preserve vendor return codes in hexadecimal error text.
- Missing dynamic symbols return `SAR_COULDNOTGETFUNCADDR` rather than panicking.
- Open device/application/container handles should be closed on every success and error path.

**Avoid in New Code:**
- Do not introduce new `unwrap()` calls on client input, locks, dynamic libraries, or serialization.
- Do not cast unvalidated user-supplied values directly to native handles.
- Prefer typed validation helpers and scoped resource guards.

## Logging

**Framework:**
- `log` plus `env_logger` for configurable log levels.
- `println!` and `eprintln!` also appear in existing startup/debug paths.

**Rules for New Code:**
- Use `log::{debug,info,warn,error}` rather than unconditional debug prints.
- Log lifecycle transitions and actionable provider/native failures with context.
- Never log PINs, private keys, session keys, plaintext cryptographic data, or unwrapped secrets.
- Prefer lengths, algorithm names, provider aliases, and redacted identifiers for diagnostics.

## Comments and Documentation

**Rust Documentation:**
- Use `///` for public functions and `//!` for module-level behavior, as demonstrated in `src/win_service.rs`.
- Explain FFI safety invariants immediately above unsafe declarations or blocks.
- Document architecture constraints such as x86 process/DLL compatibility near build code.

**Inline Comments:**
- Explain protocol quirks, cryptographic formats, cleanup behavior, and platform workarounds.
- Avoid comments that merely restate the next statement.

**JavaScript Documentation:**
- Public `SKFClient` methods use JSDoc with parameter and return descriptions.
- Keep server method name and positional parameter order synchronized with `src/main.rs`.

## Function Design

**Current Pattern:**
- `handle_request()` is a single very large match with many inline workflows and duplicated open/verify/close sequences.
- FFI wrapper methods are intentionally thin and have signatures matching the C ABI.

**Guidance for New Work:**
- Do not further grow `handle_request()` when a cohesive handler module can be extracted.
- Use typed request structs or validation helpers instead of repeated positional extraction.
- Extract resource ownership into RAII guards where possible.
- Keep cryptographic encoding helpers pure so they can be unit tested without hardware.

## Module Design

**Exports:**
- `src/skf/mod.rs` exposes `api` and `types` to the binary crate.
- `SkfApi` is the single wrapper surface over vendor libraries.
- Windows-only code is gated with `#[cfg(windows)]` at module and dependency levels.

**Boundary Discipline:**
- Keep ABI definitions isolated under `src/skf/`.
- Keep platform service management in `src/win_service.rs`.
- Keep package-building behavior under `packaging/`, not in runtime code.
- If introducing a Rust library target, move reusable protocol/domain logic to `src/lib.rs` and keep `main.rs` as composition root.

## Synchronization Requirements

When adding or changing an API method, update together:
- Server dispatch in `src/main.rs`.
- FFI wrapper/type definitions in `src/skf/api.rs` and `src/skf/types.rs` when applicable.
- Client method in `api/skf_api.js`.
- Integration coverage in `tests/test_all_apis.js`.
- Chinese and English user documentation.

---

*Convention analysis: 2026-09-11*
*Update after formatter normalization or module extraction*

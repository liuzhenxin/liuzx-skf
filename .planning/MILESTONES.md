# Milestones

## v0.3.0 Production Hardening (Shipped: 2026-09-12)

**Phases completed:** 6 phases, 20 plans, 29 tasks

**Post-ship verification (2026-09-12):**
- Windows service lifecycle UAT passed on a Windows 10 x64 VM against the
  released v0.3.0 ZIP: install → `Running (stage=serve)`, `StartPending
  (stage=provider)` observed, invalid config → `ExitCode=1066` +
  `failed/provider/code=2`, install-dir ACL enforced, upgrade over running,
  uninstall + reinstall. (`packaging/windows/uat-v030.ps1`, `Total: 4 Failed: 0`)
- Released ZIP SHA-256 independently verified on the VM.
- GitHub Release v0.3.0 published with the ZIP and checksum; `dev`/`main` CI green;
  branch protection on `main` requires the five CI checks.
- Remaining before "production-verified": real GM3000 hardware UAT (checklist B).

**Key accomplishments:**

- 37 v0.2.0 JSON-RPC methods frozen as regression fixtures recorded from the untouched pre-refactor binary, with a normalization oracle proven able to fail and zero undeclared volatility on re-record.
- The crate became a library plus a thin binary — `cargo test` went from 0 to 21 passing tests — and 15 pure encoding helpers plus the config layer moved out of `main.rs` verbatim, with behaviour preservation proven across three fresh server processes.
- The 37 hand-written request branches now have a single seam to migrate onto: a provider trait over business operations whose native implementation loads the vendor library exactly once, plus a fake that can inject every required failure and record the exact call sequence — 38 tests pass and all 37 contract fixtures still match.
- The service is now testable end to end: it reports the port the OS actually chose, takes its provider through an injection point, and replays all 37 frozen v0.2.0 fixtures against the real binary — 42 tests pass with zero warnings.
- The service now has a session ownership model: unguessable ids, a registry that tracks liveness without owning sessions, and session state held by value so a connection can actually be moved into a Tokio task.
- Authorization now belongs to one connection: a deadline per application, no PIN retained anywhere, and a process-wide cache that no longer exists.
- Clients can no longer name a native resource: every handle is a server-issued `dev-N`/`app-N`/`cnt-N`/`hsh-N` owned by one session, and abandoning a digest no longer leaks a device handle for the life of the process.
- Twelve security-relevant handlers moved out of the binary into guard-based `domain/` handlers, so a native error path can no longer skip a release; 37/37 fixtures still match and all 15 unmigrated branches are byte-identical.
- A 1 MiB frame cap, a 256 KiB pre-decode payload guard, and a 64-connection cap now bound the transport before any large allocation or vendor access; 37/37 fixtures are unchanged.
- Every vendor call — including guard `Drop` — now runs on the blocking pool behind one per-provider mutex, `WaitForDevEvent`/`CancelWaitForDevEvent` bypass the mutex, and non-wait requests time out at 30s; 37/37 fixtures still match.
- The WebSocket listener refuses any non-loopback address unless `allow_remote: true` or `SKF_ALLOW_REMOTE=1|true` is set; the refusal happens before the socket is opened.
- All 37 methods are classified ReadOnly (23) or Destructive (14), the set is verified against `EXPECTED_METHODS`, and the table is documented; nothing reads the classification at runtime.
- `cargo fmt --all -- --check` now exits 0 across the repository, delivered as a single `style(04):` commit with no logic changes.
- `cargo clippy --all-targets -- -D warnings` is green (63 → 0 warnings) and the toolchain is pinned to 1.93.0; the only surviving allow is scoped to the two functions that mirror the SKF C ABI.
- A six-job CI workflow now runs on PRs and pushes to `main`/`dev`, keeps the contract replay on an x86_64 macOS runner, and includes a job that proves a failing test turns the gate red.
- The service now reports `StartPending` per phase, reports `Running` only after the listener is bound, exits with a distinct `ServiceSpecific` code on failure, and `status` names the phase.
- The installer now locks down its directory and refuses to replace locked files, and a Windows script exercises the whole install/upgrade/uninstall lifecycle.
- Logs are now one JSON object per line, rotate at 10 MiB keeping 7 files, and a source-scan test prevents PINs, keys, or request parameters from reaching a log call.
- `skf-service diagnose` reports the five startup stages without leaking paths, the protocol accepts an optional version and answers `GetProtocolVersion`, and two safety-restricted methods can be disabled without changing the frozen contract.
- The Windows ZIP now carries a SHA-256 checksum the release workflow re-verifies, and the milestone is documented with a threat model, bilingual limits doc, release notes, and refreshed project docs.

---

# Milestones

## v0.4.0 Secure Remote Operation (Shipped: 2026-09-13)

**Phases completed:** 4 phases, 11 plans
**Git range:** `6caebb2..296c071` (`dev`)

**Key accomplishments:**

- Optional server-side TLS: a named `tls:` block (`cert_file`/`key_file`) enables TLS through `rustls` with the `ring` provider explicitly pinned (min TLS 1.2, chosen so the i686 Windows cross-build stays reproducible). With no `tls:` block the listener is plaintext loopback exactly as before, and an unreadable certificate/key is a fatal configuration error (exit code 1) with no plaintext fallback.
- Client authentication: `client_auth: mtls` installs a rustls `WebPkiClientVerifier` over a configured CA, and `token` requires `Authorization: Bearer <token>` in the WebSocket upgrade (rejected with 401 before a session exists). The token is compared in constant time and comes from `SKF_TLS_TOKEN` or a `token_file`, never inline YAML.
- One exposure rule: a non-loopback bind now requires remote opt-in **and** TLS **and** client authentication, decided before the socket is opened (exit code 3) with a message naming all three requirements; loopback stays configuration-free.
- Authorization audit: `authorization.granted`, `authorization.denied` (with reason), `authorization.expired`, and `device.unavailable` (with cleared count) are emitted from the single `SessionState` decision point as structured JSON. The event type can only carry provider/device/application/reason/count, so no PIN, key, payload, or token can be recorded.
- The authenticated identity (mTLS subject CN, `token`, or anonymous) is threaded into the session for audit and logged only as a class; the private key and token never reach logs, `diagnose`, the status file, or release metadata (diagnose exposes only `tls_enabled`/`tls_cert_loaded` booleans).
- Quality closeout: the threat model, bilingual session/limits doc, Windows service doc and release notes describe the new boundary and migration path; the Linux CI fast job now runs the hermetic `tls`/`client_auth`/`audit`/`log_redaction`/`protocol_version`/`restricted_methods` suites; Nyquist sign-off is closed for phases 3-6 (`10-NYQUIST-AUDIT.md`).

**Compatibility:** default behaviour is unchanged — plaintext loopback with v0.2.0 clients working unmodified; the 37 frozen fixtures still replay green (verified with the GM3000 token attached to the dev host).

**Known deferred items at close:** 2 human pre-release UAT flows (real operator CA + Windows service TLS lifecycle) plus documented minor tech debt; see `.planning/milestones/v0.4.0-MILESTONE-AUDIT.md`.

---

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

- Real GM3000 hardware UAT (checklist B) on the Windows 10 x64 VM: B1 session
  isolation, B2 device-removal invalidation, B3 no-PIN-retained (structural),
  B4 concurrent signing, B5 slow-op isolation all PASS. B2 exposed a real gap on
  v0.3.0 (a `ConnectDev` failure did not clear the grant); fixed in **v0.3.1** and
  re-verified on hardware (`SignData after re-insert -> -10`). B6/B7 are covered by
  automated tests; B8 awaits vendor documentation.

- v0.3.1 released (ZIP + SHA-256) and merged to `main` (PR #3).

**Status: production-verified for the tested matrix.** Remaining advisory: B6/B7
real-hardware variants, B8 vendor docs, and the phase 3-6 Nyquist VALIDATION
sign-off.

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

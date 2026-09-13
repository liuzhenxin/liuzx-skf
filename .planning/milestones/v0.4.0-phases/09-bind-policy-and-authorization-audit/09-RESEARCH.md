# Phase 9: Bind Policy and Authorization Audit — Research

**Researched:** 2026-09-12
**Note:** no `09-CONTEXT.md` exists (discuss-phase skipped). Decisions below are
grounded in the codebase and recorded for review.

## Where authorization decisions actually happen

Every authorization path funnels through `SessionState` in `src/session/mod.rs`:

| Method | Called by | Decision |
|--------|-----------|----------|
| `grant(AuthKey)` | `CheckPIN::handle` (`src/domain/pin.rs:119`) on a successful `VerifyPIN` | granted |
| `authorize(&AuthKey)` | `domain::mod::authorized` (used by 9 handlers in `keys.rs`, `container.rs`, `crypto.rs`) and `main.rs::session_authorized` | allow / `NotAuthorized` / `Expired` |
| `invalidate_device(provider, device)` | `domain::mod::note_device_unavailable` (called from 27 failure sites) and `main.rs::device_unavailable` | device-unavailable |

`AuthRejection` already models `NotAuthorized`, `Expired`, `DeviceUnavailable`.

**Decision D-01:** emit audit events from these three `SessionState` methods, not
from each call site. That guarantees complete coverage and one place to audit.
The duplicate `log::info!` lines currently in `domain::mod::note_device_unavailable`
and `main.rs::device_unavailable` are replaced by the structured event.

## Existing logging surface

`src/logging.rs` already provides a JSON-per-line rotating logger for service mode
and `env_logger` for console mode (`RUST_LOG`, default `info`). Audit events go
through `log::info!` as a JSON object string so they are:
- captured by both loggers,
- one machine-parseable record per decision,
- distinguishable by `"audit":true`.

**Decision D-02:** no new logging dependency and no second log sink. `AuditEvent::to_json`
produces the object; `audit::emit` logs it.

## Bind policy

Phase 8 added two independent checks in `bind_with_progress` (opt-in; client auth).
Phase 9 replaces them with the single exposure rule (BIND-01):

> a non-loopback bind is allowed only when **remote opt-in AND TLS enabled AND
> client authentication enabled**.

`tls.is_some()` (already computed) is the TLS half; `!client_auth.requires_loopback()`
is the auth half. The refusal keeps `StartupError::Bind` (exit code 3) so the
service manager still sees a distinct, restart-safe failure (BIND-02), and the
message names all three requirements.

**Decision D-03:** one combined check with a message that names all three
requirements, so an operator fixes the whole policy in one pass.

**Decision D-04:** loopback is untouched (no opt-in, no TLS, no auth) — the default
from v0.3.x and the 37 fixtures stay valid (BIND-03).

## Test implications

- Phase 8's `loopback_gate` positives bind `0.0.0.0` with `client_auth: token`
  and **no TLS**; under BIND-01 they must be refused. Those tests gain generated
  server cert/key (rcgen) and a new negative asserts "opt-in + client auth but no
  TLS ⇒ refused" (BIND-03).
- The audit deny path is provable end-to-end without hardware: any handler that
  calls `authorized(...)` emits `authorization.denied` when the session has no
  grant. `tests/audit.rs` captures the service's stderr with `RUST_LOG=info`.
- Grant/expiry/device-unavailable events are unit-tested through the pure
  `AuditEvent` constructors; the live line is covered by the deny path plus a
  source scan that the emit calls exist.

## Pitfalls carried into the plan

- Do not log the PIN, key material, payload, or token. Audit fields are limited to
  provider/device/application names, reason, and counts.
- Keep the existing exit codes: `Config=1`, `Provider=2`, `Bind=3`, `Serve=4`.
- Removing the two caller `log::info!` lines must not change any test's expected
  output; check `tests/log_redaction.rs` and the session tests.
- Process `RUST_LOG` defaults to `info`, so audit events are visible by default.

## Validation Architecture

- **Quick:** `cargo test --lib` (audit event shape, session decision behavior).
- **Full:** `cargo test` + `cargo check --target i686-pc-windows-gnu`.
- **Seams:** `AuditEvent::to_json`; `SessionState::{grant,authorize,invalidate_device}`;
  the bind exposure rule; console log capture.
- **Manual/hardware:** a real device removal to observe `device.unavailable` live is
  already covered by UAT B2; here it is unit-tested at the session layer.

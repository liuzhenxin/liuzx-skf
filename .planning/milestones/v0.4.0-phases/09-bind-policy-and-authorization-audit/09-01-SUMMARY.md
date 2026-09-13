---
plan: 09-01
phase: 9
status: complete
completed: 2026-09-12
requirements: [AUD-01, AUD-02]
---

# Plan 09-01 Summary: Structured Authorization Audit

## What shipped

- New `src/audit.rs`: `AuditEvent` (granted / denied / expired / device.unavailable),
  `AuditReason`, `to_json()`, `emit()`, `reason_for()`.
- Emitted from the single decision point `SessionState::{grant,authorize,invalidate_device}`:
  - `grant` → `authorization.granted`
  - `authorize` `Err(Expired)` → `authorization.expired`; other refusals →
    `authorization.denied` with `reason`; `Ok` is not logged (no per-op noise)
  - `invalidate_device` when `cleared > 0` → `device.unavailable`
- Removed the duplicate `log::info!` device-unavailable lines in
  `src/domain/mod.rs` and `src/main.rs`.

## Audit line example

```json
{"audit":true,"device":"dev-a","event":"authorization.denied","provider":"GM3000","application":"app","reason":"not_authorized"}
```

## Evidence

| Check | Result |
|-------|--------|
| `cargo test audit` (unit) | 6 passed (shape, reason, secret-free, control chars) |
| `cargo test --lib` | 145 passed |
| `grep -c 'audit::emit' src/session/mod.rs` | 4 |
| `grep -c 'log::info!' src/domain/mod.rs` | 0 |

## AUD-02

No event variant can carry a secret; string fields are run through
`logging::sanitize`; a unit test asserts the JSON never contains
`pin`/`private_key`/`payload`/`token`/`secret`.

---
phase: 03-transport-hardening-and-concurrency
plan: 03
subsystem: config
tags: [loopback, opt-in, trans-06]

requires:
  - phase: 03-01
    provides: bounded transport
provides:
  - SkfConfig.allow_remote named field and SKF_ALLOW_REMOTE env opt-in
  - bind-time refusal of non-loopback WebSocket addresses
affects: [phase-6]

tech-stack:
  added: []
  patterns:
    - "Resolve the address before deciding: loopback is judged on the socket address, not the literal string"

key-files:
  created:
    - tests/loopback_gate.rs
  modified:
    - src/config/mod.rs
    - src/server/mod.rs

key-decisions:
  - "Both YAML allow_remote and SKF_ALLOW_REMOTE opt in; either is sufficient"
  - "0.0.0.0 and :: are non-loopback and refused by default"
  - "The HTTP demo keeps its own bind defaults; the gate covers the WebSocket listener only"

requirements-completed: [TRANS-06]

duration: 30min
completed: 2026-09-11
---

# Phase 3 Plan 3: Non-Loopback Bind Opt-In Summary

**The WebSocket listener refuses any non-loopback address unless `allow_remote: true` or `SKF_ALLOW_REMOTE=1|true` is set; the refusal happens before the socket is opened.**

## Evidence

- `cargo test --test loopback_gate` → 4 passed:
  - `non_loopback_is_refused_without_opt_in` — `0.0.0.0:0` → error naming `non-loopback`, `allow_remote`, and `SKF_ALLOW_REMOTE`.
  - `loopback_is_allowed_without_opt_in` — `127.0.0.1:0` binds and reports a loopback address.
  - `non_loopback_is_allowed_with_env_opt_in` — env var admits the bind.
  - `non_loopback_is_allowed_with_yaml_opt_in` — YAML field admits the bind.
- `cargo test config` → 16 passed, including `yaml_allow_remote_true_opts_in` (which also proves the named field does not swallow the flattened providers).
- `cargo test --test contract_fixtures` → 37/37.
- `cargo test` → 127 passed.

## Notes

- `resolve_bind_addr` uses `ToSocketAddrs`, so `localhost` is judged by where it resolves and `0.0.0.0`/`::` are correctly non-loopback.
- `allow_remote` is a **named** `SkfConfig` field, so serde binds it before the flattened `libs` map — the Phase 2 D-07 trap does not apply.
- The console HTTP demo (`0.0.0.0:8000`) is unchanged by design (D-18) and remains a documented boundary.

## Self-Check: PASSED

- `grep -c 'is_loopback' src/server/mod.rs` → 1
- `grep -c 'allows_remote' src/server/mod.rs` → 1
- `grep -c 'TcpListener::bind(ws_socket)' src/server/mod.rs` → 1
- `cargo check --all-targets` → 0 warnings

---
phase: 06-observability-diagnostics-and-release-verification
plan: 01
subsystem: logging
tags: [obs-01, obs-02, logging, redaction]

requires: []
provides:
  - src/logging.rs (JSON-lines rotating logger, sanitize)
  - console/service logging initialization wired in main and win_service
  - tests/log_redaction.rs source scan
affects: [06-02, 06-03]

tech-stack:
  added: []
  patterns:
    - "log::Log implementation in-repo (no new dependency) with size rotation and bounded retention"
    - "Redaction enforced by a source-scan test, not only by convention"

key-files:
  created:
    - src/logging.rs
    - tests/log_redaction.rs
  modified:
    - src/lib.rs
    - src/main.rs
    - src/win_service.rs

key-decisions:
  - "flexi_logger could not be added (registry unreachable); the same feature set is implemented with log + serde_json"
  - "Service stdout redirects to skf-service.console.log, separate from the structured logs/skf-service.log"
  - "A log macro that names a sensitive identifier fails the scan unless it carries // redaction-allow:"

requirements-completed: [OBS-01, OBS-02]

duration: 45min
completed: 2026-09-11
---

# Phase 6 Plan 1: Structured Rotating Logs and Redaction Summary

**Logs are now one JSON object per line, rotate at 10 MiB keeping 7 files, and a source-scan test prevents PINs, keys, or request parameters from reaching a log call.**

## Forced deviation: no `flexi_logger`

CONTEXT D-01 selected an external logger crate, but `cargo add flexi_logger
--dry-run` timed out — the configured registry is not reachable, so adding a
dependency would break the build. The plan therefore implements the same feature
set in-repo with `log` + `serde_json` (both already dependencies). This is
recorded in `06-RESEARCH.md` §0, in the module docs, and in `RELEASE-NOTES.md`.

## What changed

| File | Change |
|------|--------|
| `src/logging.rs` | `RotatingFileLogger` (`log::Log`), `init_console`, `init_service_file`, `sanitize`, `MAX_LOG_BYTES=10 MiB`, `KEEP_LOG_FILES=7` |
| `src/main.rs` | `env_logger::init()` → `logging::init_console()` |
| `src/win_service.rs` | service logger under `<exe>/logs`; stdout redirect moves to `skf-service.console.log` |
| `tests/log_redaction.rs` | scans `src/` for sensitive identifiers in log/print macros; asserts request params are never logged |

## Evidence

| Check | Result |
|-------|--------|
| `cargo test logging` | 4 passed (JSON fields, rotation/retention, newline safety, sanitize) |
| `cargo test --test log_redaction` | 3 passed (baseline clean) |
| JSON line shape | `{"ts":<epoch>,"level":"INFO","target":"test","message":"hello"}` |
| Retention bound | at most `KEEP+1` files after 80 tiny-max-byte writes |
| `cargo test --test contract_fixtures` | 37/37 |
| `cargo fmt --all -- --check` | 0 |
| `cargo clippy --all-targets -- -D warnings` | 0 (one `io::Error::other` fix applied) |
| `cargo check --target i686-pc-windows-gnu` | 0 |

## Notes

- Rotation shifts `base → .1 → .2 …`, dropping the file at `keep`, then reopens
  the base file; the active list is `base` + 7 rotated files.
- `sanitize` strips control characters (so a value can never split a JSON line)
  and truncates at 512 characters.
- The scan uses word-boundary matching so `pin` does not match `pinned`/`spin`.

## Self-Check: PASSED

- 4 + 3 new tests green; 37/37 contract; fmt/clippy/i686 clean

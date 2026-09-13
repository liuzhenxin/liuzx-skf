---
plan: 10-01
phase: 10
status: complete
completed: 2026-09-12
requirements: [COMPAT-02]
---

# Plan 10-01 Summary: Documentation

## Updated

- `THREAT-MODEL.md` — v0.4.0; new "Transport security and client authentication"
  section; rewritten exposure decision (opt-in + TLS + client auth); authorization
  audit control; new/removed limitations.
- `docs/WINDOWS-SERVICE.md` — TLS/auth configuration and secret handling;
  exit code 3 now also means an exposure-rule refusal; `diagnose` TLS booleans.
- `docs/SESSION-AND-LIMITS.md` — zh+en limits row, transport-security, authorization
  audit, and v0.3.x migration sections.
- `RELEASE-NOTES.md` — new `## v0.4.0 — secure remote operation` section with
  added features, compatibility, migration, and security notes.

## Evidence

| Check | Result |
|-------|--------|
| `grep -ci 'client authentication' THREAT-MODEL.md` | 4 |
| `grep -c 'client_auth' docs/SESSION-AND-LIMITS.md` | 6 |
| `grep -c '^## v0.4.0' RELEASE-NOTES.md` | 1 |
| `grep -c 'tls_cert_loaded' docs/WINDOWS-SERVICE.md` | 1 |
| stale `No TLS` claims | 0 |

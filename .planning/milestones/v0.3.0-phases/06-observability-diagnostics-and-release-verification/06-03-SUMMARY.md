---
phase: 06-observability-diagnostics-and-release-verification
plan: 03
subsystem: release
tags: [rel-03, rel-04, rel-05, doc-01, doc-02, doc-03]

requires: [06-01, 06-02]
provides:
  - ZIP SHA-256 checksum + release-workflow verification
  - RELEASE-NOTES.md, THREAT-MODEL.md, bilingual session/limits doc
  - updated README/agent docs
affects: []

tech-stack:
  added: []
  patterns:
    - "Release artifacts are independently verifiable (hash + PE machine check on the extracted ZIP)"

key-files:
  created:
    - RELEASE-NOTES.md
    - THREAT-MODEL.md
    - docs/SESSION-AND-LIMITS.md
  modified:
    - packaging/windows/build.ps1
    - .github/workflows/release-windows.yml
    - README_CN.md
    - README_EN.md
    - AGENTS.md
    - CLAUDE.md

key-decisions:
  - "SHA-256 file uses sha256sum format and ships next to the ZIP, inside the release and as an artifact"
  - "The workflow re-verifies the extracted package rather than trusting the build step"
  - "Restricted methods default-on is documented in release notes and the bilingual limits doc"

requirements-completed: [REL-03, REL-04, REL-05, DOC-01, DOC-02, DOC-03]

duration: 50min
completed: 2026-09-11
---

# Phase 6 Plan 3: Release Integrity and Documentation Summary

**The Windows ZIP now carries a SHA-256 checksum the release workflow re-verifies, and the milestone is documented with a threat model, bilingual limits doc, release notes, and refreshed project docs.**

## Changes

| File | Change |
|------|--------|
| `packaging/windows/build.ps1` | writes `<zip>.sha256`; ships `verify-service.ps1` |
| `.github/workflows/release-windows.yml` | verifies the hash, expands the ZIP, asserts required files + `Machine=0x014C` for exe and DLL, uploads/publishes the checksum |
| `RELEASE-NOTES.md` | six intentional behavior changes with migration paths, additive surface, new capabilities |
| `THREAT-MODEL.md` | assets, trust boundary, exposure/credential decisions, controls, known limitations |
| `docs/SESSION-AND-LIMITS.md` | session model, limits, restricted methods, protocol version (中文 + English) |
| `README_CN.md` / `README_EN.md` / `AGENTS.md` / `CLAUDE.md` | post-refactor module table + current commands + diagnostics/gate references |

## Evidence

| Check | Result |
|-------|--------|
| YAML parses | `release-windows.yml` and `ci.yml` valid |
| Checksum generation | `Get-FileHash` + `.sha256` in `build.ps1` |
| Workflow verification | `Verify checksum` + extracted-content + `0x014C` steps present |
| `RELEASE-NOTES.md` | 6 migration entries, restricted methods, `GetProtocolVersion` |
| `THREAT-MODEL.md` | trust boundary, LocalSystem, TLS limitation, known limitations |
| `docs/SESSION-AND-LIMITS.md` | `## 中文` + `## English` |
| README/agent docs | `diagnose`, `diagnostic`, `cargo clippy --all-targets`, `THREAT-MODEL.md`, `i686-pc-windows-gnu` present |
| `cargo test` | 164 passed, 0 failed |
| `cargo test --test contract_fixtures` | 37/37 |
| `cargo fmt --check` / `clippy -D warnings` / i686 | 0 / 0 / 0 |

## Not executed here

The release workflow itself (tag/build) and the Windows service lifecycle were not
run: this host is macOS and the workflow requires a runner. They are recorded in
`06-HUMAN-UAT.md` / `05-HUMAN-UAT.md`.

## Self-Check: PASSED

- 164/0 tests; 37/37 contract; docs present; YAML valid; fmt/clippy/i686 clean

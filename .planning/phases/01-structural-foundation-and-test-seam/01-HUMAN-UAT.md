---
status: resolved
phase: 01-structural-foundation-and-test-seam
source: [01-VERIFICATION.md]
started: 2026-09-11T10:05:00Z
updated: 2026-09-11T14:50:00Z
---

## Current Test

[none — both items resolved]

## Tests

### 1. Windows i686 artefact still builds as PE32

expected: On a Windows build machine, `powershell -ExecutionPolicy Bypass -File .\packaging\windows\build.ps1` produces `dist\skf-service-windows-x64-gm3000-x86\skf-service.exe` with PE Machine `0x014C`, and the packaged `native\GM3000\windows\mtoken_gm3000.dll` is also `0x014C`.
result: passed 2026-09-11 — verified via Actions run 34570915581 on windows-latest (i686-pc-windows-msvc, static CRT, green in 2m46s). Downloaded the package and re-parsed the PE headers: skf-service.exe = PE32 Machine 0x014C console EXE; mtoken_gm3000.dll = PE32 Machine 0x014C DLL.

### 2. Confirm the FOUND-04 routing scope

expected: The user accepts that Phase 1 routes 4 of 37 request branches through the provider (decision D-06) and that the remaining 33 move in Phase 2 — or requests gap closure now.
result: accepted 2026-09-11 — user confirmed the scoped interpretation: FOUND-04 counts as satisfied in the "abstraction complete and fake-capable" sense; production routing of the remaining 33 branches completes in Phase 2. No gap closure requested.

## Summary

total: 2
passed: 2
issues: 0
pending: 0
skipped: 0
blocked: 0

## Gaps

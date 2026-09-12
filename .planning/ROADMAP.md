# Roadmap: LiuZX SKF Service

## Milestones

- ✅ **v0.3.0 Production Hardening** — Phases 1-6 (shipped 2026-09-12)

## Phases

<details>
<summary>✅ v0.3.0 Production Hardening (Phases 1-6) — SHIPPED 2026-09-12</summary>

- [x] Phase 1: Structural Foundation and Test Seam (4/4 plans) — completed 2026-09-11
- [x] Phase 2: Session Authorization and Resource Ownership (4/4 plans) — completed 2026-09-11
- [x] Phase 3: Transport Hardening and Concurrency (4/4 plans) — completed 2026-09-11
- [x] Phase 4: Format/Lint Normalization and CI Gate (3/3 plans) — completed 2026-09-12
- [x] Phase 5: Windows Service Reliability (2/2 plans) — completed 2026-09-12
- [x] Phase 6: Observability, Diagnostics, and Release Verification (3/3 plans) — completed 2026-09-12

Full detail (goals, requirements, per-phase success criteria, plan lists):
`.planning/milestones/v0.3.0-ROADMAP.md`

</details>

### 📋 Next milestone (planned)

Run `$gsd-new-milestone` to define the next version, requirements, and phases.

Candidate carry-over (from the v0.3.0 audit tech debt):
- Verify the Windows service lifecycle and the tag-triggered release on a real
  Windows host / CI runner (`05-HUMAN-UAT.md`, `06-HUMAN-UAT.md`).
- Bring the Linux CI `test` job's explicit test-file list up to date (phase 5/6
  test files are covered by `test-full` but not by the fast job).
- Optional Nyquist sign-off for phases 3-6 (`$gsd-validate-phase`).

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Structural Foundation and Test Seam | v0.3.0 | 4/4 | Complete | 2026-09-11 |
| 2. Session Authorization and Resource Ownership | v0.3.0 | 4/4 | Complete | 2026-09-11 |
| 3. Transport Hardening and Concurrency | v0.3.0 | 4/4 | Complete | 2026-09-11 |
| 4. Format/Lint Normalization and CI Gate | v0.3.0 | 3/3 | Complete | 2026-09-12 |
| 5. Windows Service Reliability | v0.3.0 | 2/2 | Complete | 2026-09-12 |
| 6. Observability, Diagnostics, and Release Verification | v0.3.0 | 3/3 | Complete | 2026-09-12 |

# Phase 10: Documentation and Quality Closeout — Research

**Researched:** 2026-09-12
**Note:** no `10-CONTEXT.md` (discuss-phase skipped). Decisions recorded below.

## Current documentation drift

| Document | Stale statement | Reality after v0.4.0 |
|----------|-----------------|----------------------|
| `THREAT-MODEL.md` | "No TLS, no client authentication"; "a future milestone may add TLS" | TLS + mTLS/bearer token exist; non-loopback requires all three controls |
| `docs/SESSION-AND-LIMITS.md` | non-loopback needs only `allow_remote` | needs opt-in **+** TLS **+** client auth |
| `RELEASE-NOTES.md` | latest is v0.3.1 | add v0.4.0 section |
| `docs/WINDOWS-SERVICE.md` | exit code 3 = "could not be bound" only | also the exposure-rule refusal; add TLS config |
| `docs/CI.md` / `.github/workflows/ci.yml` | fast job omits phase 7–9 suites | add `tls`, `client_auth`, `audit`, `log_redaction`, `protocol_version`, `restricted_methods` |

## Nyquist status of phases 3-6

The archived `0X-VALIDATION.md` files for phases 3, 4, 5, 6 carry
`nyquist_compliant: false` and a pending sign-off block. The work itself shipped
and was verified in the v0.3.0 milestone, but the per-task sampling/automation
sign-off was never closed. Phase 10 closes it by re-running each phase's
automated commands against the current tree and recording the mapping in
`10-NYQUIST-AUDIT.md`.

**Decision D-01:** do **not** mutate the archived milestone files (they are frozen
history). The sign-off lives in `10-NYQUIST-AUDIT.md`, which records, per phase,
its verification map, the current command, and the observed result.

**Decision D-02:** the CI fast job is the canonical hardware-free gate. Every
phase 7–9 suite that needs no token is added there. `contract_fixtures` stays a
local/pre-release check (hosted runners lack the GM3000 middleware).

## Scope of the documentation update (COMPAT-02)

- `THREAT-MODEL.md`: rewrite the trust-boundary table and exposure decision;
  document TLS (rustls/ring, min TLS 1.2), client auth (`none`/`mtls`/`token`),
  the combined bind rule, the audit events, and the new limitations.
- `docs/SESSION-AND-LIMITS.md` (zh + en): limits row for non-loopback, a
  transport-security section, an authorization-audit section, and a migration note.
- `RELEASE-NOTES.md`: add the v0.4.0 section with compatibility and migration.
- `docs/WINDOWS-SERVICE.md`: TLS/auth config, the bind refusal meaning, and the
  `diagnose` TLS booleans.

## Constraints

- The 37 v0.2.0 fixtures still pass (token attached): default is plaintext
  loopback, so COMPAT-01 holds and the docs must say so.
- Secrets must never be documented as living in YAML: the token comes from
  `SKF_TLS_TOKEN` or `token_file`; the private key is a file path, not inline.
- `docs/CI.md` and the workflow must stay in sync.

## Validation Architecture

- **Quick:** `cargo test --lib`.
- **Full:** `cargo test --no-fail-fast`, `cargo fmt --check`,
  `cargo clippy -D warnings`, `cargo check --target i686-pc-windows-gnu`.
- **Hermetic proof (QA-02):** run the TLS/client-auth/audit suites with no token
  present (`env -u SKF_TLS_TOKEN`).
- **Doc checks:** grep for the new terms and for stale claims.

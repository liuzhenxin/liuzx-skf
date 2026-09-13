---
plan: 08-01
phase: 8
status: complete
completed: 2026-09-12
requirements: [AUTH-01, AUTH-02, AUTH-03]
---

# Plan 08-01 Summary: Client-Auth Configuration and Identity Types

## What shipped

- `TlsConfig` gained `client_ca_file` (mTLS CA bundle) and `token_file` (bearer
  token file); env overrides `SKF_TLS_CLIENT_CA`, `SKF_TLS_TOKEN`,
  `SKF_TLS_TOKEN_FILE`.
- `validate_tls` now implements the full matrix:
  - `none` — always acceptable at the config layer (loopback-only at bind time);
  - `mtls` — requires server cert + key **and** `client_ca_file`;
  - `token` — requires `SKF_TLS_TOKEN` or a readable `token_file`;
  - anything else — rejected (`unknown client_auth`), never downgraded.
- `src/client_auth.rs`: `ClientAuthMode` (parse/label/requires_loopback),
  `ClientIdentity` (Anonymous/Token/Certificate+CN, `class()`), and
  `constant_time_eq` / `ClientIdentity::token_matches` (constant-time bearer
  comparison).

## Evidence

| Check | Result |
|-------|--------|
| `cargo test --lib` | 136 passed, 0 failed |
| New config tests | 6 (mtls no CA, mtls ok, token no source, token file, unknown mode, token not in error) |
| New client_auth tests | 6 (parse, unknown, requires_loopback, constant time, bearer scheme, class) |
| Token safety | `tls_token()` never appears in any error message (asserted) |

# Phase 8 Verification: Client Authentication

**Verified:** 2026-09-12 (inline execution)
**Host:** macOS; GM3000 token attached to the Windows VM

## Requirements

| Requirement | Status | Evidence |
|-------------|--------|----------|
| AUTH-01 (mTLS requires and verifies a client certificate) | PASS | `build_tls_acceptor` + `WebPkiClientVerifier`; `mtls_without_client_certificate_is_refused`, `::mtls_with_valid_client_certificate_works`, `::mtls_with_untrusted_client_certificate_is_refused` |
| AUTH-02 (bearer token required and checked before the session) | PASS | `accept_hdr_async_with_config` 401 path; `token_is_required_and_checked`, `token_mode_works_without_tls_on_loopback` |
| AUTH-03 (`none` is accepted only for a loopback bind) | PASS | `bind_with_progress` gate; `non_loopback_without_client_auth_is_refused`, `loopback_without_client_auth_still_binds` |
| AUTH-04 (identity available for audit; token/cert never logged) | PASS | `ClientIdentity` threaded via `SessionFactory::create`; `identity.class()` logged; `token_never_appears_in_diagnose_or_logs`; redaction scan |

## Commands run

```bash
cargo test --lib                 # 136 passed
cargo test --no-fail-fast        # all suites green except contract_fixtures
cargo check --target i686-pc-windows-gnu
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

## Hardware blocker (unchanged from phase 7)

`contract_fixtures` fails 22 cases, all the same `ConnectDev 0x00000001` vs the
recorded `0x00000023`, because the token is attached to the Windows VM rather than
this Mac. Dispatcher code was not modified for `ConnectDev`. Re-run with the token
on the Mac before the v0.4.0 audit.

## Notes / deviations

- The plan's token callback test names (`token_mismatch_rejects_the_upgrade`,
  `valid_token_is_accepted`) are satisfied by the end-to-end
  `tests/client_auth.rs::token_is_required_and_checked` rather than unit tests:
  they require a live listener, which the hermetic integration suite already
  provides.
- AUTH-03's "none is loopback-only" is enforced in phase 8; phase 9 (`BIND-01`)
  adds the further requirement that TLS be enabled on a non-loopback bind and
  consolidates the binding message contract.

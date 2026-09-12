---
phase: 02-session-authorization-and-resource-ownership
plan: 04
subsystem: domain
tags: [protocol-extraction, domain-migration, release-on-error, res-04, contract-preserving]

requires:
  - phase: 02-01
    provides: SessionState, SessionGuard, SessionRegistry
  - phase: 02-02
    provides: per-session authorization with TTL
  - phase: 02-03
    provides: opaque session-owned handles
  - phase: 02-04 (task 01)
    provides: protocol/ and typed Params

provides:
  - src/domain/{mod,container,crypto,keys,pin}.rs with the 12 migrated handlers
  - Provider methods create_container, gen_ecc_key_pair, gen_rsa_key_pair, begin_digest_with_key
  - PinOutcome carrying the raw vendor code
  - ServerContext trait implemented by SkfContext for alias resolution
  - 12 fake-injected release-on-error tests (RES-04 evidence)
  - tests/session_isolation.rs and tests/session_lifecycle.rs

affects: [phase-3 transport hardening, phase-6 diagnostics]

tech-stack:
  added: []
  patterns:
    - "Guard-RAII handlers: resources are provider guards, so every early return releases them"
    - "Unit-struct handlers (pub struct X; impl X { pub async fn handle(..) }) so one module can hold many branches"
    - "ServerContext trait decouples domain handlers from the binary's SkfContext and makes them testable with the fake"
    - "non-default aliases are re-expressed through NativeSkfProvider::from_api over the cached SkfApi"

key-files:
  created:
    - src/domain/mod.rs
    - src/domain/container.rs
    - src/domain/crypto.rs
    - src/domain/keys.rs
    - src/domain/pin.rs
    - tests/session_isolation.rs
    - tests/session_lifecycle.rs
    - tests/session_support/mod.rs
  modified:
    - src/main.rs
    - src/lib.rs
    - src/protocol/params.rs
    - src/provider/mod.rs
    - src/provider/native.rs
    - src/provider/fake.rs
    - .planning/phases/02-session-authorization-and-resource-ownership/02-VALIDATION.md

key-decisions:
  - "A domain handler receives a resolved provider, not a library path; alias resolution (including the four Phase-3 branches that still load directly) stays in the binary"
  - "Non-default aliases are wrapped by NativeSkfProvider::from_api over the library get_api already cached, so handler code has one shape and no library is loaded twice"
  - "PinOutcome gained a code field so CheckPIN can embed the raw vendor code in its frozen failure message"
  - "The four D-17 branches LockDev/UnlockDev/Transmit/RSAVerify keep their direct loading; the plan's 'only get_api remains' criterion is unsatisfiable within its own scope and is recorded as a deviation"
  - "session_isolation/session_lifecycle are observed at the wire level; token-dependent isolation and release are carried to 02-HUMAN-UAT.md"

patterns-established:
  - "Release-on-error tests inject one Operation failure and assert each Close* exactly once"
  - "Contract preservation proven by replaying 37 fixtures plus byte-comparing the 15 unmigrated branches"

requirements-completed: [RES-04]

duration: 3h 00min
completed: 2026-09-11
---

# Phase 2 Plan 4: Protocol Extraction, Domain Migration, and Release-on-Error Summary

**Twelve security-relevant handlers moved out of the binary into guard-based `domain/` handlers, so a native error path can no longer skip a release; 37/37 fixtures still match and all 15 unmigrated branches are byte-identical.**

## Performance

- **Duration:** 3h
- **Tasks:** 4 (1 pre-existing, 3 executed here)
- **Files:** 8 created, 7 modified

## `main.rs` size before/after

| Metric | Before (HEAD) | After |
|--------|---------------|-------|
| Lines | 3302 | 1989 |
| `Library::new` | 8 | 5 |
| `CString::into_raw()` | 56 | 19 |
| `RpcResponse::err` | 263 | 136 |
| `ctx.pins` / `hash_handles` | 0 | 0 |
| `as DEVHANDLE` | 0 | 0 |

`Library::new` fell from 8 to 5: the two key-generation branches this plan owned are gone, and the remaining five are `get_api` plus the four D-17 branches (`LockDev`, `UnlockDev`, `Transmit`, `RSAVerify`) that Phase 3 owns. `into_raw()` fell by 37 as the migrated branches moved to `c_ptr()`-style calls inside the provider guards.

## RES-04 evidence — 12 release-on-error tests

Each injects one provider `Operation` failure through `FakeSkfProvider::fail_next` and asserts the matching `Close*` was recorded exactly once.

| Test | Injected failure | Asserted releases |
|------|------------------|-------------------|
| `create_container_releases_on_native_error` | `CreateContainer` | CloseApplication, CloseDevice |
| `delete_container_releases_on_native_error` | `DeleteContainer` | CloseApplication, CloseDevice |
| `get_container_type_releases_on_native_error` | `ContainerType` | CloseContainer, CloseApplication, CloseDevice |
| `import_certificate_releases_on_native_error` | `ImportCertificate` | CloseContainer, CloseApplication, CloseDevice |
| `sign_data_releases_on_native_error` | `SignRsa` | CloseContainer, CloseApplication, CloseDevice |
| `rsa_sign_data_releases_on_native_error` | `SignRsa` | CloseContainer, CloseApplication, CloseDevice |
| `encrypt_data_releases_on_native_error` | `EncryptData` | CloseContainer, CloseApplication, CloseDevice |
| `decrypt_data_releases_on_native_error` | `DecryptData` | CloseContainer, CloseApplication, CloseDevice |
| `create_pkcs10_releases_on_native_error` | `GenEccKeyPair` | CloseContainer, CloseApplication, CloseDevice |
| `gen_ecc_key_pair_releases_on_native_error` | `GenEccKeyPair` | CloseContainer, CloseApplication, CloseDevice |
| `gen_rsa_key_pair_releases_on_native_error` | `GenRsaKeyPair` | CloseContainer, CloseApplication, CloseDevice |
| `check_pin_releases_on_native_error` | `VerifyPin` (DeviceRemoved) | CloseApplication, CloseDevice |

`check_pin_success_records_a_grant_and_releases` additionally proves the success path records exactly one grant and releases both guards; the no-PIN-retained property is pinned by `session::auth::tests::a_grant_holds_only_a_deadline`.

## Contract evidence

- `cargo test --test contract_fixtures` → **37 compared, 0 failed** (4 harness tests, including `fixture_count_is_at_least_37`).
- The 15 unmigrated branches were extracted by brace matching from `HEAD:src/main.rs` and the working tree and compared byte-for-byte: **all IDENTICAL** — `SetLanguage`, `EnumProvider`, `EnumApplication`, `EnumContainer`, `FindCertificates`, `GetDevInfo`, `SetLabel`, `ECCVerify`, `RSAVerify`, `LockDev`, `UnlockDev`, `Transmit`, `Digest`, `IssueCertificate`, `ImportKeyPair`.

## Phase 2 success criteria — conclusions

1. **PIN verified by one session does not authorize another** — ✅ `session::tests::two_sessions_do_not_share_authorization` (handler level); wire-level adversarial case deferred to manual (no token).
2. **Authorization expires and clears on disconnect/device removal** — ✅ `session::auth::tests::expiry_removes_the_grant`, `session::tests::session_clears_grant_when_device_reported_removed`; disconnect teardown smoke-tested by `tests/session_lifecycle.rs`.
3. **No recoverable PIN survives verification** — ✅ `session::auth::tests::a_grant_holds_only_a_deadline` (grant is a `Copy` deadline, no `String`); `ctx.pins` count is 0.
4. **Opaque identifiers only; unknown/expired/wrong-kind rejected** — ✅ `session::handles` tests plus `tests/session_isolation.rs` (fabricated integer and guessed handle both `-11`).
5. **Native resources released on disconnect, error paths, and session end; digest state not shareable** — ✅ 12 RES-04 tests + `dropping_the_table_releases_every_resource` + `digest_handle_is_released_when_the_session_ends`; cross-session digest unusability asserted at the wire level.

## 待人工验证 (manual verification required)

1. **真实设备下的会话隔离 (SESS-02 增强)** — 有 GM3000 的机器上：会话 A `CheckPIN` 成功后，会话 B 的 `SignData` 必须被拒。本机无 token，工厂只走拒绝路径。
2. **设备拔出后的失效行为 (SESS-04b)** — A 授权后拔出 Key，A 的下一次敏感操作须以 device-not-found 被拒且授权条目被清除。
3. **Windows i686 产物 (D-25)** — 本机仅 `cargo check --target i686-pc-windows-gnu` 通过（0 警告）；`release-windows.yml` 需在 CI 触发并以 `Machine=0x014C` 为证据。
4. **真实设备上的释放** — 拔出/断连后断言 `Close*` 实际调用；本机通过 fake 注入证明，真实 token 未验证。

## Deviations from Plan

**[Rule 3 - Missing Wave 0 artifact] `tests/session_isolation.rs` and `tests/session_lifecycle.rs` did not exist**
- Found during: Task 04
- Issue: `02-VALIDATION.md` lists both as Wave 0 requirements and task 02-04-04 requires them to pass, but neither file was present.
- Fix: created both plus a shared `tests/session_support/mod.rs` harness. Because the machine has no token, they assert the observable rejection/isolation contract and leave token-dependent cases to manual verification.
- Verification: `cargo test --test session_isolation` → 2 passed; `cargo test --test session_lifecycle` → 2 passed.

**[Rule 1 - Unsatisfiable criterion] `grep -c 'Library::new' src/main.rs` cannot reach 0 or 1**
- Found during: Task 03 verification
- Issue: the plan criterion expects only `get_api` to remain, but D-17 keeps `LockDev`, `UnlockDev`, `Transmit`, and `RSAVerify` on the direct path through Phase 3, and each loads its own library. Forcing the criterion would mean migrating branches the plan explicitly defers.
- Fix: none; recorded here. Current count is 5 (`get_api` + the four D-17 branches), and `grep -c 'Library::new' src/domain/*.rs` is 0.
- Verification: `grep -n 'Library::new' src/main.rs` lists exactly those five sites.

**[Rule 1 - Provider surface gap] Provider lacked container creation and key generation**
- Found during: Task 02/03
- Issue: `ApplicationGuard` had no `create_container`, and `ContainerGuard` had no key-pair generation; `begin_digest` could not pass the SM2 public key `CreatePKCS10` needs.
- Fix: added `create_container`, `gen_ecc_key_pair`, `gen_rsa_key_pair`, and `begin_digest_with_key` to the trait with native and fake implementations; added `EccPublicKey`/`RsaPublicKey` value types so no native handle crosses the boundary.
- Verification: `tests/provider_invariants.rs` still passes; `cargo check --target i686-pc-windows-gnu` passes.

**Total deviations:** 3 (2 auto-fixed, 1 documented as unsatisfiable-within-scope). **Impact:** the security objective (RES-04) is fully met; the one unmet grep criterion reflects a plan-scope inconsistency, not a missing release guarantee.

## Issues Encountered

- The `ImportCertificate` branch does not fold the literal `"default"` alias into the configured default (matching the pre-refactor code). The first release test used `"default"` and correctly failed authorization; the test now passes the literal alias, and the behaviour was left unchanged to avoid smuggling a fix into the move.

## Self-Check: PASSED

- `cargo test` → lib 85 + contract 4 + oracle 4 + provider invariants 6 + session invariants 6 + session isolation 2 + session lifecycle 2 = **109 passed**, 0 failed, 1 ignored
- `cargo check --all-targets` → 0 warnings
- `cargo check --target i686-pc-windows-gnu` → success
- `cargo test --test contract_fixtures` → 37/37
- `grep -c 'pub async fn handle' src/domain/{container,crypto,keys,pin}.rs` → 4 / 5 / 2 / 1
- `grep -c 'Library::new' src/domain/*.rs` → 0
- 15 unmigrated branches byte-identical to HEAD

---
phase: 01
slug: structural-foundation-and-test-seam
status: issues_found
depth: standard
reviewed_at: 2026-09-11
scope_files: 12
scope_excluded_fixtures: 41
critical: 0
high: 0
medium: 4
low: 6
threats_open: 0
---

# Phase 1 — Code Review

**Scope:** source and test files changed between the phase base (`ecb5158`, the
v0.2.0 release commit) and `3fc2fa9`.

Reviewed files (12):

- `Cargo.toml`
- `src/lib.rs`, `src/main.rs`, `src/win_service.rs`
- `src/config/mod.rs`, `src/crypto/mod.rs`
- `src/provider/mod.rs`, `src/provider/native.rs`, `src/provider/fake.rs`
- `src/server/mod.rs`
- `tests/common/mod.rs`, `tests/contract_fixtures.rs`, `tests/fixture_oracle.rs`

Excluded from review: the 41 files under `tests/fixtures/v0.2.0/` (recorded data,
not code — reviewed for secrets instead, clean) and `.planning/` artefacts.

## Summary

No critical or high findings. The phase's structural goals hold: the provider
boundary names no native type, the vendor library is loaded once in
`provider/native.rs`, guards release in `Drop`, and the crate still contains
exactly one `unsafe impl Send`/`Sync`.

Four medium and six low findings follow. Three of the medium findings are
abstraction-completeness issues that Phase 2 will hit directly, so they are worth
resolving before the session work builds on the seam.

## Medium

### M-01 — `begin_digest` requires the concrete device type, so it is unreachable through the trait

**File:** `src/provider/native.rs` (`begin_digest`), `src/provider/mod.rs`

`pub fn begin_digest(device: &Arc<NativeDevice>, ...)` takes the concrete type,
while `SkfProvider::open_device` returns `Box<dyn DeviceGuard>`. Any caller
holding a trait object therefore cannot start a digest — the only way in is to
downcast to a type the boundary deliberately hides.

**Why it matters:** Phase 2 migrates the `DigestInit`/`DigestUpdate`/
`DigestFinal`/`CloseHash` branches, which is exactly the case that cannot be
expressed today.

**Suggested fix:** put `begin_digest` on `DeviceGuard`
(`fn begin_digest(&self, alg_id: u32, public_key: Option<&[u8]>, id: &[u8]) -> ProviderResult<Box<dyn DigestGuard>>`),
which also removes the need for the free function.

### M-02 — `wait_for_event` allocates from a caller-supplied length

**File:** `src/provider/native.rs` (`SkfProvider::wait_for_event`)

```rust
let len = if buf_len == 0 { DEV_NAME_BUF_LEN } else { buf_len };
let mut buf = vec![0u8; len];
```

`buf_len` reaches an allocation unchecked. The one production caller passes
`256`, so this is not currently exploitable, but the trait is public and the
allocation is attacker-influenced wherever a caller forwards a client value.

**Why it matters:** Phase 3 adds request limits (TRANS-01); this is the kind of
site that has to be bounded before that work is meaningful. It also contradicts
the module's own rule of not trusting caller input.

**Suggested fix:** clamp to the vendor maximum, e.g.
`let len = buf_len.clamp(MIN, DEV_NAME_BUF_LEN)`, or drop the parameter and always
use `DEV_NAME_BUF_LEN`.

### M-03 — 33 branches still load the vendor library directly

**File:** `src/main.rs`

`src/main.rs` still contains 11 `Library::new` call sites (ten request arms plus
`SkfContext::get_api`). Several of those arms load the library, use it, and drop
it while native handles created earlier remain live — the latent defect research
identified (Pitfall 1/4, and the reason `NativeSkfProvider` holds one
`Arc<Library>`).

**Why it matters:** this is a known pre-existing defect, not a regression, but it
is the largest remaining correctness risk in the service and it lives in the same
file the next phase edits.

**Status:** deliberately out of scope for Phase 1 (decision D-06). Recorded here
so Phase 2 does not rediscover it.

### M-04 — `provider_load_failed` returns a response with no id, patched by callers

**File:** `src/main.rs` (`provider_load_failed` and its three call sites)

```rust
fn provider_load_failed(err: &ProviderError, lang: &Language) -> RpcResponse {
    RpcResponse::err(-5, msg, None)   // id: None
}
```

Every caller must remember to write the id back:

```rust
let mut resp = provider_load_failed(&e, lang);
resp.id = id;
resp
```

A future caller that forgets produces a response with no `id`, which a JSON-RPC
client cannot correlate. The compiler will not catch it.

**Suggested fix:** take `id: Option<serde_json::Value>` as a parameter and return
a complete response.

## Low

### L-01 — `UnavailableProvider` puts an error message in a field named `path`

**File:** `src/server/mod.rs`

```rust
ProviderError::LibraryLoadFailed {
    path: self.detail.clone(),      // actually a pre-formatted error string
    arch: std::env::consts::ARCH,
    detail: self.detail.clone(),
}
```

The rendered message therefore contains the detail twice, and `path` does not
hold a path. Cosmetic today because no fixture covers the un-loadable-default
case, but it makes the failure text misleading exactly when an operator needs it.

### L-02 — Two independent handle-name spaces

**File:** `src/provider/native.rs`

`NativeSkfProvider` issues device/app handles from its own `AtomicU64` starting at
1; nested resources (applications, containers, digests) come from a
process-global `NEXT_NESTED_HANDLE` static starting at 1_000_000. Two counters
mean two allocation policies for one concept, and the static is shared across
provider instances.

**Suggested fix:** route nested handle allocation through the provider so all
handles come from one counter. Phase 2 replaces these values with session-scoped
opaque ids, which is the natural moment to do it.

### L-03 — Fixed 64 KiB response buffers per symmetric call

**File:** `src/provider/native.rs` (`SYM_BUF_LEN`)

`encrypt`, `decrypt`, and `transmit` each allocate 64 KiB per call regardless of
payload size. Correct but wasteful under load; Phase 3's limit work will need a
size policy anyway.

### L-04 — `spawn_client` has no per-connection guard

**File:** `src/server/mod.rs`

Every accepted TCP stream spawns a task with no cap on concurrent connections and
no frame-size check. Consistent with the pre-refactor behaviour and explicitly
deferred (TRANS-01/TRANS-02), recorded here because the new `server` module is
where the limit will be enforced.

### L-05 — Error messages returned to clients include the resolved library path

**File:** `src/main.rs` (`provider_load_failed`), `src/provider/mod.rs`

`ProviderError::LibraryLoadFailed` renders the absolute path. This is preserved
deliberately — the recorded v0.2.0 contract asserts the message — and the listener
is loopback-only. It remains information disclosure toward any local caller and
should be revisited when the contract is allowed to change (OBS-04 in Phase 6).

### L-06 — `contract_fixtures.rs` duplicates `EXPECTED_METHODS` knowledge

**File:** `tests/contract_fixtures.rs`

`every_expected_method_has_a_fixture` compares the fixture set against
`common::EXPECTED_METHODS`, which is a hand-maintained list. Adding a method
without updating the list leaves the new method unguarded while the test still
passes.

**Suggested fix:** derive the expected list from the dispatcher's match arms, or
add a check that the count of arms in `src/main.rs` equals the list length.

## Verification of Phase-1 Invariants

| Invariant | Command | Result |
|-----------|---------|--------|
| One `unsafe impl Send`/`Sync` | `cargo test --test provider_invariants` | pass |
| No native type across the boundary | `cargo test --test provider_invariants` | pass |
| Library loaded once in the provider | `cargo test --test provider_invariants` | pass |
| Guards release in `Drop` | `cargo test --test provider_invariants` | pass |
| Fake not in release builds | `cargo test --test provider_invariants` | pass |
| No secrets in fixtures | `grep -rE 'BEGIN .*PRIVATE KEY|12345678' tests/fixtures/` | clean |
| Contract preserved | `cargo test --test contract_fixtures` | 37 compared, 0 failures |

## Threat Model Review

All four per-plan threat-model entries are mitigated and verified:

| Threat | Status | Evidence |
|--------|--------|----------|
| T-01-01 fixtures generated by the fake (circular proof) | Mitigated | `tools/record_fixtures.js` imports nothing from `src/`; fixtures recorded from `ecb5158` before any production change |
| T-01-02 post-hoc `normalize` widening | Mitigated | Declarations shipped with the recordings; the one later addition (`EnumProvider` → `<unordered>`) is documented in `provenance.volatile_reasons` and in the README |
| T-01-03 native pointer crossing the provider boundary | Mitigated | Enforced by `provider_boundary_exposes_no_native_pointers`; guard fields are integers |
| T-01-04 over-compliant fake | Mitigated | 10 fake tests, all five failure kinds asserted, including a blocking-delay case |

No open threats.

## Recommendation

Fix **M-01** and **M-04** before starting Phase 2 — both are cheap and both are on
the path the next phase walks. M-02 and M-03 belong to Phase 3 and Phase 2
respectively and are already recorded there. The six low findings can ride along
with whichever phase touches those files next.

---

*Review completed: 2026-09-11*
*Reviewer: inline review (no subagent runtime available)*

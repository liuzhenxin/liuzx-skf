---
phase: 01
slug: structural-foundation-and-test-seam
status: all_fixed
iteration: 1
fix_scope: critical_and_warning
findings_in_scope: 2
fixed: 2
skipped: 0
out_of_scope_low: 6
fixed_at: 2026-09-11
---

# Phase 1 — Code Review Fixes

Scope: `critical_and_warning` (the default). `01-REVIEW.md` reported 0 critical,
0 high, 4 medium, 6 low; the two medium findings that sit directly on Phase 2's
path were fixed. The other two medium and all six low findings are recorded below
as deliberately deferred.

## Fixed

### M-01 — `begin_digest` was unreachable through a trait-object device

**Commit:** `7f8aa85`

**Problem.** `begin_digest` was a free function taking `&Arc<NativeDevice>`,
while `SkfProvider::open_device` returns `Box<dyn DeviceGuard>`. A caller holding
a trait object therefore could not start a digest at all — precisely the case
Phase 2 hits when it migrates the `DigestInit`/`DigestUpdate`/`DigestFinal`/
`CloseHash` branches.

**Fix.**

- Moved `begin_digest` onto the `DeviceGuard` trait:
  `fn begin_digest(&self, alg_id: u32, id: &[u8]) -> ProviderResult<Box<dyn DigestGuard>>`
- Implemented it for both the native and the fake guard.
- Removed both provider-level entry points (`native::begin_digest` and
  `FakeSkfProvider::begin_digest`) so there is exactly one way in.
- Dropped the unused SM2 public-key parameter — no caller passes one — along with
  the `ECCPUBLICKEYBLOB` import it required, keeping the trait free of native types.

**Verification.** A new regression test obtains the device exactly as production
does and starts a digest:

```rust
let device: Box<dyn DeviceGuard> = provider.open_device("dev-a").expect("open");
let digest = device.begin_digest(0x00000001, b"").expect("...");
```

This test could not have been written before the change. `cargo test --lib` → 29
passed (the 10 fake tests now go through the guard path), `provider_invariants`
→ 6 passed, no warnings.

### M-04 — `provider_load_failed` returned a response with no `id`

**Commit:** `549455d`

**Problem.** The helper returned `RpcResponse::err(..., None)` and each of the
three call sites patched the id back by hand:

```rust
let mut resp = provider_load_failed(&e, lang);
resp.id = id;
resp
```

A future caller that forgot would produce a response a JSON-RPC client cannot
correlate, and nothing would catch it.

**Fix.** The helper now requires the id:
`fn provider_load_failed(err: &ProviderError, lang: &Language, id: Option<serde_json::Value>) -> RpcResponse`.
All three call sites pass it, and the manual patching is gone
(`grep -c 'resp.id = id' src/main.rs` → 0).

**Verification.** `cargo test` → 43 passed. Behaviour preserved: all 37 fixtures
still MATCH under oracle normalization after both fixes.

## Skipped — deliberately deferred

| Finding | Severity | Why deferred |
|---------|----------|--------------|
| M-02 `wait_for_event` allocates from a caller-supplied length | medium | Belongs with Phase 3's request-limit work (TRANS-01); the one production caller passes 256 |
| M-03 33 branches still load the vendor library directly | medium | Explicitly Phase 2 scope (decision D-06); the provider seam is now ready for them |
| L-01 `UnavailableProvider` puts a message in a field named `path` | low | Cosmetic; no fixture covers the un-loadable-default case |
| L-02 two independent handle-name spaces | low | Phase 2 replaces these values with session-scoped opaque ids anyway |
| L-03 fixed 64 KiB response buffers | low | Phase 3's limit work needs a size policy regardless |
| L-04 `spawn_client` has no per-connection guard | low | Deferred as TRANS-01/TRANS-02; the new `server` module is where it lands |
| L-05 error messages include the resolved library path | low | Preserved deliberately — the recorded contract asserts the message |
| L-06 `EXPECTED_METHODS` is hand-maintained | low | Worth fixing when a method is next added |

## Not Fixed — out of scope by design

`--all` was not used, so Info-level findings were not in scope. There were none
in any case; the six deferred items above are Warning-class at most.

## Post-Fix State

| Check | Result |
|-------|--------|
| `cargo test` | 43 passed (29 lib, 4 oracle, 4 contract, 6 invariant), 0 failed |
| `cargo check --all-targets` | 0 warnings |
| `cargo check --target i686-pc-windows-gnu` | success |
| Behaviour preservation | 37/37 fixtures MATCH |
| Open critical/high findings | 0 |

No re-review loop was run (`--auto` not used); the single fix pass is complete.

---

*Fixes applied: 2026-09-11*

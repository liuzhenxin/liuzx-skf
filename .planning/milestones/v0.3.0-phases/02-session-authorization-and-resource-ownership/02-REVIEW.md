---
phase: 02
phase_name: session-authorization-and-resource-ownership
status: clean
depth: standard
findings_total: 4
high: 0
medium: 0
low: 4
fixed: 4
reviewed_at: 2026-09-11
reviewer: inline (no subagent runtime available)
---

# Phase 2 Code Review

Scope: the files plan 02-04 changed — `src/domain/*.rs`, `src/main.rs`,
`src/lib.rs`, `src/protocol/params.rs`, `src/provider/{mod,native,fake}.rs`,
and the two new session integration tests.

The two blocking hazards from the phase handoff were applied: the scripted
12-branch replacement was read hunk by hunk, and bare greps were backed by
comment-aware checks (the unmigrated branches were byte-compared by brace
extraction rather than trusted to a grep).

## Findings

### L-1 (fixed) — Present-but-empty `symKey` was treated as absent

`EncryptData`/`DecryptData` used `params.optional_str(4, "")`, which cannot tell
an absent key from `""`. The pre-refactor code decoded `""` to a zero-byte key
and rejected it with `SM4 key must be 16 bytes`. The migration silently accepted
it and proceeded without a key. Fixed by reading `params.raw(4)` and decoding
whatever string is present.

### L-2 (fixed) — `IVLen` was clamped to the array width

The provider's `block_cipher_param` set `IVLen = min(iv.len(), 32)`, while the
pre-refactor branch set `IVLen = iv_bytes.len()` and copied at most 32 bytes.
For an IV longer than 32 bytes this changed the vendor argument. Fixed in
`src/provider/native.rs` to carry the declared length.

### L-3 (accepted) — Fixed-width slice assumptions in `SignData`/`CreatePKCS10`

`sign_ecc` is trusted to return 128 bytes and `device.random(8)` 8 bytes; a
provider that returns short would panic. The native and fake implementations
both honour the widths, the boundary is internal, and the pre-refactor code used
fixed-size `ECCSIGNATUREBLOB` arrays, so this preserves the existing risk rather
than adding one. No change.

### L-4 (accepted) — `unsafe` in `domain::struct_bytes`

`struct_bytes` uses `slice::from_raw_parts` on a POD blob to reproduce the exact
wire encoding of `GenECCKeyPair`/`GenRSAKeyPair`. It has a safety comment, the
blob is a live local, and the size is `size_of::<T>()`. No change.

## Security notes

- No new `unsafe impl Send/Sync`; `tests/provider_invariants.rs` passes.
- `domain/` names no native handle type in the provider boundary sense; the only
  native-struct usage is the POD key blobs in `mod.rs`/`crypto.rs`, matching
  `crypto/mod.rs`.
- All twelve handlers verify authorization through `SessionState`; the
  `check_pin` handler stores only a grant, and the `Grant` size/Copy-ness is
  pinned.
- `Library::new` does not appear in `src/domain/`.

## Verification

- `cargo test` → 109 passed, 0 failed
- `cargo test --test contract_fixtures` → 37/37
- `cargo check --all-targets` → 0 warnings
- `cargo check --target i686-pc-windows-gnu` → success

# Phase 3: Transport Hardening and Concurrency — Research

**Researched:** 2026-09-11
**Method:** source-level analysis of this repository plus the vendored crate sources
(`tungstenite-0.21.0`, `tokio-tungstenite-0.21.0`) in the local cargo registry. No
external web fetch was available in this runtime; every claim below is backed by a
file path or a command output in this repo.

**Question answered:** "What do I need to know to PLAN this phase well?"

---

## Summary of constraints carried in

| Constraint | Source | Consequence for this phase |
|------------|--------|----------------------------|
| `SkfConfig` uses `#[serde(flatten)]` for providers; an **unnamed** top-level key is captured as a provider and can break parsing | 02-CONTEXT D-07; `src/config/mod.rs:22-26` | New config must be a **named** field or an env var |
| `WaitForDevEvent` must not run on the async worker, and `CancelWaitForDevEvent` must be able to run **while** the wait is in progress | 01-VERIFICATION; `src/main.rs:336-340` | A global lock must **exempt** the wait/cancel pair or cancel deadlocks |
| Exactly one `unsafe impl Send` and one `unsafe impl Sync` | `src/skf/types.rs:39-40`; `tests/provider_invariants.rs` | Serialization must be expressed with `Arc<Mutex<()>>`, never a new `unsafe impl` |
| 37 v0.2.0 fixtures must keep matching; `normalize` is frozen | `tests/fixtures/v0.2.0/README.md` | New errors (timeout, payload/connection rejection) must not perturb fixture-covered paths |

---

## 1. Transport surface today

`src/server/mod.rs`:

- `spawn_client` performs `accept_async(stream)` with **no** `WebSocketConfig`, so
  tungstenite's defaults apply: `max_message_size = 64 MiB`,
  `max_frame_size = 16 MiB` (verified in `tungstenite-0.21.0/src/protocol/mod.rs:82-83`).
- There is **no connection counter or semaphore**; `listener.accept()` spawns a task per
  connection unconditionally.
- `bind` parses `opts.ws_addr` only inside `TcpListener::bind`; there is no address
  classification before binding (`src/server/mod.rs:243-262`).

**Available hooks (verified in vendored source):**

- `tokio_tungstenite::accept_async_with_config(stream, Some(config))` exists
  (`tokio-tungstenite-0.21.0/src/lib.rs:137`).
- `WebSocketConfig { max_message_size: Option<usize>, max_frame_size: Option<usize>, ..Default::default() }`
  (`tungstenite-0.21.0/src/protocol/mod.rs:34-83`). The limit is enforced during frame
  read / message assembly (`:550`, `:622`), i.e. **before** the full payload is
  materialised.

## 2. Payload decode today

`src/protocol/params.rs` decodes base64 in `required_base64` / `optional_base64`
without a length precheck. `base64` 0.22 allocates the decoded buffer on decode, so a
huge encoded string is validated only after allocation. A precheck on the **encoded**
length is the allocation guard:

```
max_encoded = max_decoded * 4 / 3 + 4   // base64 padding/rounding headroom
```

`params.rs` is in the library crate and has 12 unit tests; adding a limit is a
library-level change and directly testable.

## 3. Blocking FFI surface

Provider methods are **synchronous** and call the vendor C ABI. They are reached from:

- `src/domain/*.rs` — the 12 migrated handlers (async fns with **no real `.await`**).
- `src/main.rs` — the 14 D-17 branches (EnumApplication, FindCertificates, RSAVerify,
  LockDev, UnlockDev, Transmit, IssueCertificate, ImportKeyPair, etc.).
- `WaitForDevEvent` / `CancelWaitForDevEvent` — already split between
  `spawn_blocking` (default provider) and a direct call (cancel), `src/main.rs:340/365/1729`.

**Established pattern:** `tokio::task::spawn_blocking(move || provider.method(...)).await`.
Guards are `Box<dyn …Guard>` and all guard traits are `Send`, so a guard can be moved
into the closure and returned alongside the result. This is the pattern to generalise.

**Critical ordering constraint (D-06):** a `std::sync::Mutex` held across a
synchronous call must be taken on a blocking thread. Introducing the lock while calls
still run on async workers would block Tokio worker threads on the mutex. Lock and
`spawn_blocking` must land in the **same plan/commit**.

## 4. Thread-safety of the vendor library

No vendor thread-safety statement is available. Evidence that concurrency is already
dangerous:

- `src/provider/mod.rs` documents: "serialisation of non-thread-safe vendor calls is
  the implementation's responsibility".
- `SkfApi` holds no lock; a single `Arc<SkfApi>` is shared by all connections
  (`build_sessions`, `SkfContext::provider`).
- `SKF_CancelWaitForDevEvent` is explicitly process-global and is invoked while a wait
  blocks on the same library — proof that the vendor expects at least this one pair to
  run concurrently, and therefore that a blanket lock would change semantics.

**Decision driver:** D-01/D-02 (per-provider global lock, wait/cancel exempt).

## 5. Wait/cancel semantics (the lock's hard boundary)

`WaitForDevEvent` blocks inside the vendor call until an event or the vendor's
"device not found"-style error. `CancelWaitForDevEvent` must reach the library while
the wait is parked. Consequences:

1. Neither may be wrapped by the new global lock.
2. Neither may be given the 30 s timeout (they are intentionally long-lived).
3. `CancelWaitForDevEvent` currently runs on an async worker (`src/main.rs:1729`); per
   TRANS-04 it should also move to `spawn_blocking` so a slow cancel cannot occupy a
   worker. This is safe because cancel does not take the lock.

## 6. Non-loopback opt-in surface

- Addresses come from `SKF_WS_ADDR` (default `127.0.0.1:9001`) and `SKF_HTTP_ADDR`
  (`src/server/mod.rs:64-70`).
- `SkfConfig` fields: `default: String`, `libs: HashMap` (flattened), `vendor: HashMap`
  (`src/config/mod.rs:22-30`). Adding `allow_remote: Option<bool>` is a **named** field,
  so serde binds it before the flatten map — D-07 is not violated.
- `bind` is the single place to validate before `TcpListener::bind`.
- `RunMode::Console`'s HTTP default is `0.0.0.0:8000`; per D-18 it stays out of scope.

## 7. Operation classification surface

- The canonical method list lives in `tests/common/mod.rs` (`EXPECTED_METHODS`) and is
  asserted by `tests/contract_fixtures.rs::every_expected_method_has_a_fixture`.
- There is no existing enum mapping methods to behavior class. `src/domain/mod.rs` is
  the natural home, or a new `src/domain/classification.rs`.
- A classification test can iterate `EXPECTED_METHODS` (available to integration tests
  via `tests/common`) or a library-side constant. To keep the test in the library,
  define the method list / classifier in the library and have the integration test
  compare it to `EXPECTED_METHODS`.

---

## Pitfalls

1. **Implementing the lock before `spawn_blocking`** (D-06) → worker threads park on
   the mutex; the "stalled client" failure gets worse, not better.
2. **Locking wait/cancel** (D-02) → cancel queues behind wait; the pair never completes.
3. **Enforcing limits after allocation** → the guard must run before `base64::decode`
   and before tungstenite assembles the message.
4. **Breaking the fixtures with a new error code** for timeout/payload/connection
   rejection. Fixtures only exercise normal rejection paths; new errors must not
   replace existing ones for the same inputs.
5. **Widening `normalize`** to absorb an unexpected drift — that is a contract change
   and must be handled as one.
6. **`0.0.0.0`/`::` treated as loopback** → the wildcard bind is the most dangerous
   case and must be refused by default.
7. **New `unsafe impl` to make guards share the lock** → forbidden; the lock is
   `Arc<Mutex<()>>`, which is already `Send + Sync`.

---

## Validation Architecture

This section feeds `03-VALIDATION.md`.

**Framework:** Rust built-ins — `#[test]` / `#[tokio::test]`; reuse the library test
target and the existing subprocess harness (`tests/contract_fixtures.rs::start_service`,
`tests/session_support/mod.rs`).

**Quick run:** `cargo test --lib`
**Full suite:** `cargo test` then `cargo check --all-targets` and
`cargo check --target i686-pc-windows-gnu`
**Feedback latency:** ≈ 7 s for `cargo test` on this machine.

**Test seams this phase adds:**

| Seam | Where | What it lets a test prove |
|------|-------|---------------------------|
| Frame/message limit | `src/server/mod.rs` (`accept_async_with_config`) | An over-limit frame is rejected and the process survives |
| Payload limit | `src/protocol/params.rs` (unit tests) | Over-limit encoded input rejects with `-2` **without** decoding; boundary at exact limit passes |
| Connection cap | `src/server/mod.rs` (`Semaphore`) | The 65th concurrent connection is refused while 64 are held |
| Global FFI lock | `src/provider/native.rs` + fake call recording | Two concurrent operations on one provider do not interleave; wait/cancel are exempt |
| `spawn_blocking` isolation | `FakeSkfProvider::fail_next(Blocking(..))` | A slow provider call does not stall an unrelated request (e.g. `SetLanguage`) |
| Timeout | provider call wrapper | A call exceeding 30 s returns `-1` with the documented message |
| Loopback gate | `bind` / `SkfConfig.allow_remote` | Non-loopback without opt-in fails before bind; with opt-in succeeds |
| Classification | `OperationClass` + `EXPECTED_METHODS` | Every method has exactly one class |

**Sampling:** after each task run `cargo test --lib`; after each wave run `cargo test`
plus the i686 cross-check; before `$gsd-verify-work` the full suite and
`contract_fixtures` must be green.

**Manual-only:** true concurrency interleaving on a real GM3000 (no token here);
whether the vendor DLL tolerates concurrent calls outside the lock (cannot be tested
without the DLL's own concurrency bug reproducing).

---

## Recommended task/test surface (input to planning)

- **TRANS-01:** `WebSocketConfig` limits + `Params` payload precheck; unit tests for the
  exact boundary; end-to-end test that a 2 MiB frame is rejected.
- **TRANS-02:** `Semaphore(64)` in `serve`; test holds N connections and asserts N+1 is
  dropped.
- **TRANS-03/04:** `spawn_blocking` wrapper + 30 s timeout; a `Blocking` fake proves an
  unrelated request still completes; wait/cancel exempt.
- **TRANS-05:** `Arc<Mutex<()>>` in `NativeSkfProvider`, taken in every native method;
  an integration/invariant test proves consecutive calls are serialized (fake records
  order) and that no `unsafe impl` was added.
- **TRANS-06:** `allow_remote` named field + `SKF_ALLOW_REMOTE`; `bind` precheck; tests
  for refuse/allow paths.
- **TRANS-07:** `OperationClass` + coverage test against `EXPECTED_METHODS`; docs table.

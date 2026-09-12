# Pitfalls Research — v0.3.0 Production Hardening

**Domain:** Rust native-FFI PKI device gateway, security refactor, Windows service operations
**Researched:** 2026-09-11
**Confidence:** HIGH (grounded in this codebase's observed state and well-established FFI/service failure modes)

## Critical Pitfalls

### Pitfall 1: Refactoring a security service before any test exists

**What goes wrong:**
`main.rs` is decomposed into modules while the test count is still zero. Behavior changes silently: a method stops closing a container, a parameter order shifts, an error code changes. Nothing detects it until hardware UAT, and by then the refactor is too large to bisect.

**Why it happens:**
The module split feels like a prerequisite for tests, so teams do the whole refactor first and add tests "after". The refactor also touches cryptography paths where bugs are invisible without independent verification.

**How to avoid:**
- Land the split in small, reviewable steps, each keeping the binary behaviorally identical.
- Add tests **with** each extracted piece — pure DER/base64/config logic first, since those need no hardware.
- Freeze the v0.2.0 JSON contract as a set of golden request/response fixtures **before** touching dispatch, then assert against them throughout.
- Keep `IssueCertificate`'s OpenSSL path and all crypto output formats byte-identical unless the change is the explicit subject of a phase.

**Warning signs:**
- A commit that moves hundreds of lines with no accompanying test.
- "Tests will be added once the structure settles."
- No golden-fixture file in the repo.

**Phase to address:**
Structural foundation phase (the first phase), gated by a frozen protocol fixture set and a non-zero test count.

---

### Pitfall 2: Session isolation that leaks through the provider layer

**What goes wrong:**
Sessions are introduced at the protocol layer, but the provider/worker layer still caches device or application handles globally (as `SkfContext.apis` and `hash_handles` do today). Session A's teardown then closes a handle Session B is still using, or Session B inherits Session A's still-open application context.

**Why it happens:**
Authorization is the visible requirement, so teams fix `pins` and consider the work done. Handle ownership is a deeper, less obvious coupling — and the current code already has a `hash_handles` map that is process-global and never cleaned on disconnect.

**How to avoid:**
- Decide explicitly what is process-scoped (the loaded library) versus session-scoped (device/application/container/hash handles).
- Every handle must have exactly one owner; on teardown, close only owned handles and invalidate them in the registry before closing.
- Test the adversarial case directly: two sessions, overlapping devices, one disconnects — assert the other still works.
- Treat device removal as a third invalidation source alongside expiry and disconnect.

**Warning signs:**
- Any handle stored in a process-wide map without an owner field.
- Session teardown implemented as "clear the whole map".
- No test with two concurrent sessions.

**Phase to address:**
Session and resource-ownership phase.

---

### Pitfall 3: Treating a security refactor as a behavior-preserving refactor

**What goes wrong:**
The hardening changes are folded into the structural refactor. Reviewers cannot tell whether a diff changed structure or changed security semantics, and a subtle authorization bypass lands inside a "move code" commit.

**Why it happens:**
Both changes touch the same functions, and doing them together feels efficient.

**How to avoid:**
- Separate phases: structure first (provably behavior-preserving, verified by fixtures), then behavior (security semantics, verified by new adversarial tests).
- Security-semantics changes must have tests that **fail before** the change.
- Document the intended behavior delta for every method whose authorization or handle semantics change.

**Warning signs:**
- A commit described as "refactor" that changes an authorization check.
- No failing-test-first evidence for a security fix.

**Phase to address:**
Enforced as a sequencing rule between the structural phase and the session phase.

---

### Pitfall 4: Breaking the v0.2.0 client contract during hardening

**What goes wrong:**
Response shapes, parameter ordering, or method names change as part of "cleanup". Field deployments that already ship `api/skf_api.js` stop working after a service upgrade, with no diagnosis path.

**Why it happens:**
Parameter access gets rewritten as typed deserialization, and it is natural to "fix" inconsistencies (for example the non-standard `{error, result, message, id}` envelope) at the same time.

**How to avoid:**
- Freeze the v0.2.0 request/response shapes as fixtures before refactoring.
- Add protocol version negotiation rather than changing existing shapes.
- If a method must be restricted for safety, keep the method present and return a clear, documented error code rather than removing it.
- Update `api/skf_api.js`, both READMEs, and the test suite in the same change as any intentional delta.

**Warning signs:**
- Typed DTOs whose field names differ from the current JSON keys.
- A changelog entry that does not mention breaking changes.
- Client and server updated in different commits.

**Phase to address:**
Protocol/versioning phase, with the freeze established in the structural phase.

---

### Pitfall 5: Assuming native handles are safe to share across async tasks

**What goes wrong:**
Handles are moved into `tokio::spawn`ed tasks or shared behind `Send + Sync` wrappers because it compiles. Two tasks then call the vendor DLL concurrently on the same device handle, producing intermittent corruption, wrong-signature results, or a hard crash inside the vendor library — untraceable from Rust.

**Why it happens:**
`SendHandle` already exists and `unsafe impl Send + Sync` silences the compiler. Vendor documentation about thread-safety is usually absent or vague.

**How to avoid:**
- Assume vendor libraries are **not** thread-safe unless proven otherwise; serialize per-device access in the provider layer.
- Keep `unsafe impl Send/Sync` in exactly one place with a documented invariant, and never add a second one.
- Use a per-device worker/actor so ownership and serialization are structural, not convention.
- Add a concurrency test with a fake provider that fails if two operations overlap on one device handle.

**Warning signs:**
- More than one `unsafe impl Send`/`Sync` in the crate.
- A vendor handle reachable from more than one task.
- Intermittent hardware errors that disappear under a debugger or when logging is added.

**Phase to address:**
Provider boundary phase.

---

### Pitfall 6: Clearing a PIN by overwriting a Rust `String`

**What goes wrong:**
The team "zeroizes" the PIN by writing zeros over a `String` or `Vec`, believing the secret is gone. The allocator may have already copied or moved the buffer, and `String` growth/reallocation leaves earlier copies behind. The secret persists in process memory.

**Why it happens:**
The requirement says "don't store the PIN", and a well-intentioned zeroing pass is added without understanding Rust ownership and reallocation.

**How to avoid:**
- The correct behavior is to not retain the PIN at all after verification: parse into a buffer, call VERIFY, drop immediately, store only a boolean/expiry per tuple.
- If a buffer must be zeroed, use an opt-in zeroizing container and never build it from `String`/`format!`.
- Never log, format, or include the PIN in an error message or panic payload.
- Record the decision in the threat model so reviewers know retention is deliberate, not forgotten.

**Warning signs:**
- A PIN-typed `String` field anywhere outside a short-lived local.
- Comments like "zeroed for security" on a plain `String`.
- `format!` calls that interpolate credential values.

**Phase to address:**
Session and resource-ownership phase, verified by a test asserting the credential is absent from any persistent structure.

---

### Pitfall 7: Reporting Windows service "Running" before the service is actually usable

**What goes wrong:**
`Running` is reported before config parsing and listener binding complete (the current behavior). The SCM and any monitoring see a healthy service while every request fails. Worse, initialization failure exits with code 0, so `sc.exe failure` restart policy never triggers and the service stays silently dead.

**Why it happens:**
The status-reporting call is placed at the top of the service entry point because that is the simplest place, and the failure path reuses the same `Win32(0)` exit code as the success path.

**How to avoid:**
- Report `StartPending` with a wait hint while initializing; report `Running` only after the listener is bound and the configured provider has been resolved.
- Return a distinct non-zero service-specific exit code for each initialization failure class.
- Verify with `sc query` timing and a deliberately broken config: the service must fail, and the configured restart action must actually fire.
- Keep startup failures visible in `skf-service.log` with the precise failing stage.

**Warning signs:**
- `set_service_status(Running)` appearing before any fallible initialization.
- A single exit-code constant used for both success and failure.
- No test that starts the service with an invalid config.

**Phase to address:**
Windows service reliability phase.

---

### Pitfall 8: Unbounded logs, frames, and payloads

**What goes wrong:**
The service appends to one log file forever and accepts arbitrarily large WebSocket frames. A deployed gateway eventually fills a disk or is OOM-killed by a single malformed client — and even loopback clients can do this accidentally.

**Why it happens:**
Local-only scope creates a false sense that limits are unnecessary.

**How to avoid:**
- Bound frame size, payload size, connection count, per-operation duration, and log retention explicitly.
- Check size **before** allocating or decrypting.
- Rotate logs with a retention count; verify pruning by generating synthetic volume.
- Choose limits from observed real payload sizes (certificate and key blobs), not guesses.

**Warning signs:**
- No size constants anywhere in the transport layer.
- Log output that includes full payloads.
- A single append-only log path with no rotation config.

**Phase to address:**
Transport hardening phase; log rotation can land earlier with the logging work.

---

### Pitfall 9: Letting the CI gate land before the codebase is clean

**What goes wrong:**
A workflow that runs `cargo fmt -- --check` and `cargo clippy -- -D warnings` is added to a codebase with 198 formatting diffs and ~72 Clippy warnings. Every pull request is red. The team starts ignoring the gate or disables it, and the hardening release ships with no real merge check.

**Why it happens:**
Enabling CI is treated as an independent, obviously-good task, and its dependency on a clean baseline is overlooked.

**How to avoid:**
- Normalize formatting in an isolated commit with no logic changes; verify the diff is whitespace-only.
- Reduce Clippy to zero actionable warnings, using explicit, documented allows for specification-required FFI naming.
- Only then enable the gate, and start it in a non-blocking/report-only mode for one cycle if the team needs it.
- Keep the gate's failure output actionable.

**Warning signs:**
- A first CI run with hundreds of failures.
- Blanket `#![allow(...)]` added to make the gate pass.
- Gate disabled "temporarily" and never re-enabled.

**Phase to address:**
Format/lint normalization phase, immediately before the CI phase.

---

### Pitfall 10: Unsafe or incomplete uninstall/upgrade paths on Windows

**What goes wrong:**
Upgrade reinstalls over a running service, leaving a locked executable or a stale service registration pointing at an old path. Uninstall removes the service but leaves the install directory, logs, or an orphaned registration that blocks the next install. The installer also copies the package into a directory with inherited permissions, so a non-privileged user can replace the DLL that LocalSystem loads.

**Why it happens:**
Install is thoroughly tested because it is the visible path; upgrade, uninstall, and directory ACLs are exercised only when something already went wrong.

**How to avoid:**
- Test the full lifecycle explicitly: install → run → upgrade over running instance → uninstall → reinstall.
- Make install idempotent and ensure uninstall is complete (service, files, logs or documented retention).
- Apply explicit ACLs to the install directory so only administrators can modify the payload.
- Verify the installed service's executable path after upgrade, not just that the service is "Running".

**Warning signs:**
- No upgrade test in the release checklist.
- Install directory inheriting default `Program Files` ACLs silently assumed sufficient for a LocalSystem-loaded DLL.
- Uninstall leaving files behind without documentation.

**Phase to address:**
Windows service reliability phase and release-verification phase.

---

### Pitfall 11: Health/diagnostic surface that leaks sensitive or identifying data

**What goes wrong:**
The health command is designed for debugging convenience and reports device serial numbers, absolute install paths, provider file paths, and the last error message verbatim — which may embed a PIN or key material from a failed operation. Anyone with local access now has hardware inventory and potentially credentials.

**Why it happens:**
"Local only" is treated as equivalent to "safe to be verbose".

**How to avoid:**
- Define an allow-list of reportable facts: booleans, provider aliases, port numbers, uptime, error **class** codes.
- Never echo raw error strings or file paths that contain user identity.
- Keep the health surface loopback-only and read-only.
- Test that a deliberately PIN-containing failure does not appear in health output.

**Warning signs:**
- Health output containing file paths or serial numbers.
- Reusing unredacted log/error text as the health payload.
- A health endpoint that accepts parameters.

**Phase to address:**
Diagnostics/health phase.

---

### Pitfall 12: Fake provider that is too compliant

**What goes wrong:**
The fake SKF backend always succeeds and returns well-formed data. The CI suite is green, but the actual failure paths — wrong PIN, retry exhaustion, missing symbol, device unplugged mid-operation, native call blocking forever, cleanup after partial success — are never exercised. The dangerous code stays untested while appearing covered.

**Why it happens:**
Building a fake to make tests pass is easier than building one to make tests fail meaningfully.

**How to avoid:**
- Give the fake configurable failure injection from the start: per-operation error codes, delays, disconnection events.
- Require that each hardening requirement has at least one test where the fake **fails**.
- Assert cleanup and release after failures, not just successful results.
- Include a blocking-call simulation to prove the runtime is not starved.

**Warning signs:**
- Fake with no error-injection API.
- Test count high but all assertions are "no error".
- No test asserting handle release after a failure.

**Phase to address:**
Provider boundary and test-infrastructure phases.

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Keep `unsafe impl Send/Sync` on handles | Avoids actor/worker plumbing | Undefined behavior with non-thread-safe vendor DLLs; unreproducible crashes | Only while a documented single-owner invariant holds and serialization is enforced |
| Store authorization as a cached PIN string | Minimal change to existing `pins` map | Credential recovery from memory; shared across connections; no expiry | Never for v0.3.0 — replace with boolean/expiry per session |
| Single append-only service log | Trivial implementation | Disk exhaustion on a long-running service | Only with rotation and retention configured |
| Blanket `#![allow(clippy::all)]` to get a green gate | Fast CI enablement | Hides real defects; gate becomes meaningless | Never — use targeted, documented allows |
| Leave `IssueCertificate` inconsistent for double certs | Saves certificate-generation work | Wrong material installed if anyone treats it as real | Acceptable only while labeled mock-only and excluded from production docs |
| Defer the `serde_yaml` replacement | Avoids a parser migration | Deprecated dependency persists | Acceptable for v0.3.0 with explicit follow-up milestone |
| Hand-written positional parameter parsing | Less code now | Inconsistent validation, weak types, no payload limits | Only for methods not touched by the refactor; migrate opportunistically |
| Test only on hardware | Highest fidelity | No PR gate; regressions found late; requires a physical token | Never as the only strategy — pair with fake-provider CI |

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| Vendor SKF DLL | Assuming thread-safety because it compiles | Serialize per device; one worker owns each native handle |
| `libloading` symbol resolution | Treating missing symbols as crashes | Preserve `SAR_COULDNOTGETFUNCADDR` mapping and test it with a fake |
| `windows-service` SCM | Reporting Running before initialization completes | StartPending with wait hint → Running only after bind; non-zero failure exit codes |
| Windows installer | Reinstalling over a running service | Stop → wait for removal (already present) → replace → start; verify path after upgrade |
| OpenSSL CLI subprocess | Treating it as a core dependency | Keep isolated to mock certificate flow; degrade with a clear error if absent |
| `%VAR%` path expansion | Silently leaving unexpanded placeholders | Validation must report unresolved variables and whether the resolved file exists |
| WebSocket client (`ws`) | Server and client updated in different commits | Same-commit updates plus a compatibility test against the frozen protocol fixtures |
| GitHub Actions Windows runner | Assuming hardware or drivers exist | Fake-provider tests only; hardware UAT on a separate machine (documented) |
| Tokio runtime | Running blocking FFI directly in async tasks | `spawn_blocking` or a dedicated worker; bound the queue and operation duration |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Blocking FFI on async worker threads | Unrelated connections stall while one token operation runs; accept loop stalls | `spawn_blocking` / provider worker pool | Immediately with a slow token or a blocking `WaitForDevEvent` |
| Global lock around all native access | Throughput collapses to one operation at a time across all providers | Serialize per device/provider instance, not per process | With 2+ devices or concurrent clients |
| Buffering whole payloads before validation | Memory spike and possible OOM on a hostile/malformed client | Size-check before allocate/decrypt | At any attacker-chosen payload size |
| Rebuilding `SkfApi`/re-resolving symbols per call | Latency and inconsistent library lifetime | Single provider runtime; loaded library cached once | Immediately on high-frequency operations |
| Unbounded `hash_handles` growth | Memory growth plus stale native handles | Session-scoped registry with expiry and release | After many digest sessions without cleanup |
| Unbounded log volume | Disk exhaustion, slow writes | Rotation, retention, level control | On long-running services or debug logging in production |

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Process-wide authorization state | Any client inherits another client's authorization | Per-session authorization with TTL and clear-on-disconnect |
| Client-supplied native handle | Crash, undefined vendor behavior, cross-session object access | Opaque server-issued IDs with kind and ownership validation |
| Readable device strings from fixed buffers | Out-of-bounds read if vendor omits NUL termination | Bounded scan of the fixed array; never `CStr::from_ptr` on unvalidated memory |
| LocalSystem service loading a user-replaceable DLL | Privilege escalation via DLL replacement | Explicit install-directory ACLs; least-privilege service account where driver access allows |
| Unauthenticated destructive methods on a reachable port | Container deletion, key overwrite, device lock by any local process | Loopback default enforced in code; classify destructive methods; refuse non-loopback without explicit opt-in |
| Secrets in health output or logs | Credential/inventory disclosure | Allow-list health fields; never log or format credentials; add a regression test |
| `unwrap()` on client input | Remote/local denial of service via panic | Typed validation returning protocol errors; panic-free request path |
| No payload limits | Memory exhaustion, oversized crypto operations | Bound frame/payload/operation size before processing |

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| Generic provider load failure ("Load Library Failed") | Operators cannot fix it without reading source | Report provider, OS, resolved path, file existence, process architecture, OS error code |
| Silent config mistakes surfacing at first request | Failure appears unrelated to configuration | Validate at startup and fail fast with the offending key |
| Service "Running" but non-functional | Monitoring lies; troubleshooting starts from zero | Accurate readiness plus a health command that names the failing stage |
| Error messages as free-form strings | Clients cannot handle errors programmatically | Stable error codes plus localized messages |
| Bilingual messages implemented per branch | Inconsistent language coverage across methods | Localize at one rendering point with a complete message table |
| Breaking client changes without migration notes | Existing integrations break silently | Version negotiation, deprecation notes, and same-commit docs/client updates |

## "Looks Done But Isn't" Checklist

- [ ] **Session isolation:** Often missing handle ownership — verify two overlapping sessions, one disconnecting, the other still functional.
- [ ] **PIN handling:** Often missing absence verification — verify no persistent structure retains credential bytes after verification.
- [ ] **Opaque handles:** Often missing kind validation — verify a container ID rejected where a device ID is expected.
- [ ] **Resource release:** Often missing failure-path release — verify handles released after native errors, not just success.
- [ ] **Provider errors:** Often missing diagnostics — verify a deliberately wrong path produces an actionable message naming provider/OS/path/architecture.
- [ ] **Service readiness:** Often missing failure case — verify an invalid config yields non-zero exit and triggers the configured restart action.
- [ ] **Health output:** Often missing redaction — verify a PIN-containing failure does not appear in health or logs.
- [ ] **Log rotation:** Often missing pruning proof — verify old files are actually deleted, not just created.
- [ ] **Limits:** Often missing pre-allocation checks — verify an oversized frame is rejected without large allocation.
- [ ] **Fake provider:** Often missing failure injection — verify at least one test per requirement where the fake returns an error.
- [ ] **CI gate:** Often missing actual enforcement — verify a deliberately failing test blocks merge.
- [ ] **Release artifact:** Often missing content verification — verify extracted ZIP contains expected files and a DLL with `Machine=0x014C`.
- [ ] **Protocol compatibility:** Often missing regression proof — verify frozen v0.2.0 fixtures still pass after refactoring.
- [ ] **Uninstall:** Often missing completeness — verify service, files, and registration removed, and reinstall succeeds.

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Refactor without tests | HIGH | Freeze fixtures from the last released binary, restore behavior, rebase the refactor on tests |
| Cross-session handle interference | MEDIUM | Add ownership tracking, invalidate on teardown, add the two-session regression test |
| v0.2.0 client broken by upgrade | HIGH | Ship compatibility shim immediately, add fixtures, document migration |
| Vendor crash from concurrent handle use | HIGH | Serialize per device, add overlap-detection test, pin a reproducible repro |
| Service silently dead after failed start | MEDIUM | Fix exit code, add readiness gating, verify with invalid-config test |
| Disk full from logs | MEDIUM | Truncate/rotate immediately, add retention, review what is logged |
| DLL replaced by non-admin user | HIGH | Re-apply directory ACLs, reinstall with hardened installer, audit host |
| Red CI gate ignored | MEDIUM | Normalize baseline, re-enable with targeted allows, require the check before merge |
| Secret found in logs or health output | HIGH | Purge logs, rotate any exposed credential (re-PIN if required), add redaction test |

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| 1. Refactor without tests | Structural foundation | Frozen protocol fixtures + non-zero test count before refactor complete |
| 2. Session isolation leaks | Session/resource ownership | Two-session overlap test; ownership fields asserted |
| 3. Mixed refactor + semantics | Sequencing rule (structural → session) | Security changes have failing-test-first evidence |
| 4. v0.2.0 contract broken | Protocol/versioning | Frozen v0.2.0 fixtures pass unchanged |
| 5. Unsafe handle sharing | Provider boundary | Overlap-detection test; single documented `unsafe impl` |
| 6. PIN "zeroization" theater | Session/resource ownership | Assertion that no persistent structure retains credentials |
| 7. Early Running status | Windows service reliability | Invalid-config test: non-zero exit + restart fires |
| 8. Unbounded logs/frames/payloads | Transport hardening (+ logging phase for rotation) | Oversized frame rejected pre-allocation; log pruning verified |
| 9. Red CI on first run | Format/lint normalization → CI | First CI run green; gate blocks a deliberately failing test |
| 10. Unsafe install/upgrade/uninstall | Windows service reliability + release verification | Full lifecycle test including upgrade over running service and ACL check |
| 11. Leaky health surface | Diagnostics/health | PIN-containing failure absent from health output |
| 12. Over-compliant fake provider | Provider boundary + test infrastructure | Each requirement has at least one fake-failure test |

## Sources

- `.planning/codebase/CONCERNS.md` — observed defects and risks in this codebase
- `.planning/codebase/ARCHITECTURE.md`, `.planning/codebase/TESTING.md`, `.planning/codebase/CONVENTIONS.md`
- Direct inspection of `src/main.rs`, `src/win_service.rs`, `src/skf/api.rs`, `src/skf/types.rs`, `packaging/windows/*`, `.github/workflows/release-windows.yml`
- Established Rust FFI/async and Windows service operational failure modes
- **No external post-mortem or upstream documentation research performed during this pass**

---
*Pitfalls research for: LiuZX SKF Service v0.3.0*
*Researched: 2026-09-11*

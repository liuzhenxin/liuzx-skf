# Release Notes

## v0.3.0 — production hardening

v0.3.0 hardens the SKF gateway: session-scoped authorization, opaque resource
handles, bounded transport, deterministic resource release, a real merge gate, an
honest Windows service, and non-sensitive diagnostics. The v0.2.0 JSON-RPC client
continues to work unmodified.

Compatibility: the 37 frozen v0.2.0 request/response fixtures are replayed on every
run and still match. Two additions (`apiVersion`, `GetProtocolVersion`) are
documented below as **additive** and change no recorded response.

---

## Intentional behavior changes

Each entry states the change, why it was made, and how to migrate.

### 1. Resource handles are opaque and server-issued

- **Change:** `ConnectDev` returns a string handle (`dev-1`, `app-1`, `cnt-1`,
  `hsh-1`) instead of the decimal value of a native pointer. `DisConnectDev` and
  `GenerateRandom` reject a bare integer or an unknown handle with `-11`.
- **Why:** a fabricated native handle was observed to kill the service process.
- **Migration:** store and return the handle string from `ConnectDev`. Do not parse
  it as a number. A client that previously sent an integer gets `-11` instead of a
  crash.

### 2. Non-loopback binding is refused by default

- **Change:** the WebSocket listener refuses a non-loopback address unless the
  configuration sets `allow_remote: true` or the environment sets
  `SKF_ALLOW_REMOTE=1`.
- **Why:** the protocol has no transport authentication; the loopback default is the
  security boundary.
- **Migration:** bind loopback (the default). To expose remotely, opt in explicitly
  and place the service behind a trusted proxy/network control.

### 3. Transport and operation limits

- **Change:** WebSocket messages and frames are capped at 1 MiB; an operation
  payload is capped at 256 KiB (checked before base64 decoding); concurrent
  connections are capped at 64 (excess connections are refused); a non-`WaitForDevEvent`
  request times out after 30 s (`SKF_FFI_TIMEOUT_SECONDS` overrides it).
- **Why:** bound memory and keep one stalled token from occupying the runtime.
- **Migration:** keep individual payloads below 256 KiB. A timeout stops the caller
  waiting; it does not cancel the vendor call.

### 4. The Windows service reports startup truthfully

- **Change:** the service reports `StartPending` while initializing and `Running`
  only after the listener is bound. A startup failure exits with a distinct
  service-specific code (`1` config, `2` provider, `3` bind, `4` serve) and writes
  `service-state.json`. `status` prints the phase.
- **Why:** "process running" and "service usable" are different, and a clean exit
  never triggered restart-on-failure.
- **Migration:** monitor `skf-service.exe status` / `service-state.json`; the
  installer's `sc failure` action now fires on startup failure.

### 5. Provider resolution: fatal vs degraded

- **Change:** an invalid configuration (unparseable YAML, unknown default alias, no
  path for the current OS) is fatal at startup. A configured path whose library is
  missing or fails to load degrades to `UnavailableProvider`, and requests return
  the established `Load Lib Failed` error.
- **Why:** a build or CI host without the vendor library must still be able to start
  the service.
- **Migration:** fix the configuration to resolve provider path errors; a missing
  library is a runtime condition, not a configuration error.

### 6. Restricted methods can be disabled

- **Change:** `IssueCertificate` and `Transmit` remain implemented and work by
  default. With `SKF_RESTRICT_LEGACY=1` they return `-100` with a documented
  message.
- **Why:** safety-sensitive methods stay present (never disappear) but an operator
  can turn them off.
- **Migration:** leave the variable unset for current behavior; set it in hardened
  deployments and update callers to handle `-100`.

---

## Additive (v0.3.0) protocol surface

- Requests may carry an optional `"apiVersion": 1`. A v0.2.0 client does not send
  it and is treated as version 1; a higher value returns `-1` with a documented
  message.
- `GetProtocolVersion` returns `{"min":1,"current":1,"service":"0.3.0"}`. It is an
  explicit contract addition and has no v0.2.0 fixture (see
  `tests/common/mod.rs::ADDITIVE_METHODS`).

## New capabilities

- `skf-service diagnose [--config <path>] [--json]` — local, non-sensitive startup
  diagnostics (config, provider, library presence/load, listener, uptime).
- Structured JSON-line logs under `<install>/logs/skf-service.log`, rotated at
  10 MiB keeping 7 files. The Windows service's stdout/stderr go to a separate
  `skf-service.console.log`. The logger is implemented in-repo (no new dependency).
- The installer applies and verifies a restrictive ACL on the install directory
  (`SYSTEM`/`Administrators` full, `Users` read+execute).
- The Windows release ZIP ships a SHA-256 checksum file, and the release workflow
  verifies the hash plus the extracted package and `Machine=0x014C`.
- Every method has a documented read-only/destructive classification
  (`docs/OPERATION-CLASSIFICATION.md`).

## Documentation

- `THREAT-MODEL.md` — trust boundary, exposure and credential decisions, known
  limitations.
- `docs/SESSION-AND-LIMITS.md` — session/authorization model, limits, restricted
  methods (Chinese and English).
- `docs/CI.md`, `docs/WINDOWS-SERVICE.md` — merge gate and Windows service.

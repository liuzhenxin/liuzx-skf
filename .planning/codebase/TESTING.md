# Testing Patterns

**Analysis Date:** 2026-09-11

## Test Framework

**Rust Runner:**
- Cargo's built-in test harness.
- No `#[test]` or `#[tokio::test]` functions currently exist.
- `cargo test` succeeds while running zero tests.

**JavaScript Runner:**
- Custom executable Node scripts; no Jest, Vitest, Mocha, or assertion package.
- `tests/test_all_apis.js` tracks passed/failed/skipped counts with local `check()` and `skip()` helpers.
- `tests/test_usb_event.js` is an interactive long-running event observer rather than an automated bounded test.

**Run Commands:**
```bash
cargo test
cargo check --target i686-pc-windows-gnu
node --check api/skf_api.js
node --check tests/test_all_apis.js
./tests/run_tests.sh
SKF_NO_RESTART=1 SKF_WS_URL=ws://127.0.0.1:9001 ./tests/run_tests.sh
node tests/test_usb_event.js
```

**Quality Commands:**
```bash
cargo fmt -- --check       # Currently fails against baseline formatting
cargo clippy --all-targets # Succeeds with substantial warning output
```

## Test File Organization

**Location:**
- Hardware integration tests are in the separate `tests/` directory.
- JavaScript client code under test is `api/skf_api.js`.
- There are no Rust integration files (`tests/*.rs`) or source-local unit-test modules.

**Current Structure:**
```text
tests/
├── run_tests.sh          # Build/deploy/start/run/cleanup orchestration
├── test_all_apis.js      # Consolidated API and hardware workflow checks
└── test_usb_event.js     # Continuous insertion/removal listener
```

**Naming:**
- Test scripts use `test_*.js`.
- The orchestration entry uses `run_tests.sh`.
- Several files named in `README_CN.md` no longer exist, so documentation is stale.

## Test Structure

**Consolidated API Pattern:**
```javascript
try {
    const result = await skf.enumDevice(provider);
    check('EnumDevice / 枚举设备', result.length > 0);
} catch (e) {
    check('EnumDevice / 枚举设备', false, e.message);
}
```

**Patterns:**
- One `SKFClient` connects to a real running WebSocket service.
- Setup discovers provider, device, application, and containers dynamically.
- Tests stop early or mark cases skipped when no physical device/container exists.
- Certificate tests create uniquely named containers and attempt cleanup at the end.
- A global 60-second timer forces process exit if the suite hangs.
- Bilingual labels are used in console reports.

## Setup and Teardown

**Setup:**
- `tests/run_tests.sh` builds the debug Rust binary.
- It copies the binary into `dist/skf-service-macos-x86_64`, starts it, and waits for port 9001.
- It installs `ws` with npm if the module is missing.
- Environment variables select PIN and WebSocket URL.

**Teardown:**
- Created certificate/key containers are deleted near the end of `tests/test_all_apis.js`.
- The Bash wrapper kills only the service process it started.
- Early returns and forced exits can leave hardware containers or temporary files behind.

**Platform Limitation:**
- `tests/run_tests.sh` uses `killall`, `lsof`, Unix paths, and a macOS-specific dist directory; it is not a Windows service test harness.

## Mocking

**Framework:**
- None.

**Current Approach:**
- Most tests use a real USB Key, real native library, real WebSocket server, and real token state.
- `IssueCertificate` is a runtime mock CA implemented by the service with local OpenSSL, not a test double injected by the test suite.

**Recommended Boundaries for Future Tests:**
- Extract provider operations behind a trait and use a fake SKF backend for deterministic API tests.
- Unit test pure DER/base64/config/path functions without native middleware.
- Keep a smaller hardware certification suite for vendor-specific behavior.
- Mock SCM/package operations only at the process/script boundary; retain at least one Windows VM installation UAT.

## Fixtures and Test Data

**Current Data:**
- Inline fixed byte buffers and base64 payloads.
- Subject DNs such as `CN=TestSingle,O=Test,C=CN`.
- Container names based on timestamps.
- Wrong-PIN and configured-PIN behavior is tested against real hardware.

**Shared Fixtures:**
- None.
- Mock CA key/certificate files are generated in the OS temp directory by the running service.

## Coverage

**Requirements:**
- No line, branch, or function coverage target.
- No coverage tool configuration.
- GitHub Actions does not run the Rust or Node test suite.

**Observed Baseline:**
- `cargo test`: 0 passed, 0 failed, 0 ignored because no Rust tests exist.
- Node syntax checks pass for `api/skf_api.js` and both current test scripts.
- Hardware integration results depend on local device, driver, PIN, OpenSSL, and token contents.

## Test Types

**Unit Tests:**
- Missing.
- High-value candidates: environment expansion, DER encoding, subject parsing, config validation, RPC request validation, PE parser, and service state transitions.

**Integration Tests:**
- `tests/test_all_apis.js` covers enumeration, PIN language behavior, digest, CSR, mock issuance, certificate import, key import, signing, SM4 round-trip, locking, transmit, key generation, RSA paths, and streaming hash operations.
- Assertions are mostly shape/truthiness checks; cryptographic correctness is not always verified independently.

**Hardware/System Tests:**
- Physical token and driver are required.
- `tests/test_usb_event.js` validates device insertion/removal manually.
- There is no automated Windows SCM install/start/stop/reboot/uninstall UAT.

**Release Tests:**
- `packaging/windows/build.ps1` validates that the GM3000 DLL and produced executable are PE32/i386.
- `.github/workflows/release-windows.yml` verifies ZIP creation and uploads it.
- The workflow does not launch the executable, validate package contents, or install the service.

## Known Weak Assertions

- RSA verification in `tests/test_all_apis.js` uses the generated signature as a placeholder public-key blob, so it does not prove valid RSA verification behavior.
- Many API checks assert only boolean success or non-empty output.
- No negative boundary suite covers malformed JSON, oversized payloads, invalid handles, missing symbols, corrupted certificates, or concurrent clients.
- Cleanup is not guaranteed with `finally`, making repeated hardware runs less deterministic.

## Adding Tests

**Pure Rust Logic:**
- Extract into modules and add source-local `#[cfg(test)] mod tests` blocks.
- Use table-driven cases for malformed and boundary inputs.

**Protocol Tests:**
- Start the server on ephemeral ports with a fake provider.
- Assert complete response shape, error code, ID correlation, and connection isolation.

**Hardware Tests:**
- Mark clearly as opt-in and require explicit environment variables.
- Record provider model, firmware, driver, and architecture in result output.
- Use guaranteed cleanup guards for created applications/containers.

**Windows Service Tests:**
- Run on an isolated Windows VM/runner with elevated permissions.
- Verify install, readiness, status, stop, restart, failure recovery, log creation, and uninstall.

---

*Testing analysis: 2026-09-11*
*Update when unit-test infrastructure or CI gates are added*

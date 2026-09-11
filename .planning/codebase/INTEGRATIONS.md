# External Integrations

**Analysis Date:** 2026-09-11

## Hardware and Native Middleware

**SKF-compatible USB Keys / HSMs:**
- The service calls vendor implementations of the Chinese SKF C ABI through `src/skf/api.rs`.
- GM3000 is the active default and packaged Windows provider; paths are defined in `config/skf.yaml`.
- Provider lookup can use configured VID:PID mappings through the `EnumProvider` JSON-RPC method.
- Hardware authentication uses application PIN verification; PIN values are supplied by API clients and cached in process memory.

**Vendor Libraries:**
- GM3000 Windows: `native/GM3000/windows/mtoken_gm3000.dll` (PE32/i386).
- GM3000 macOS: `native/GM3000/macos/x86_64/libgm3000.1.0.dylib`.
- FishMan Windows: `native/FishMan/windows/KeyGDBApi.dll`.
- 3000GM paths exist in `config/skf.yaml`, but matching binaries are not committed and no package validates them.
- Native symbols are resolved lazily by `libloading`; missing symbols become SKF return code `SAR_COULDNOTGETFUNCADDR`.

**External Driver Requirement:**
- Bundled user-mode DLLs do not replace vendor USB/HID/kernel drivers.
- Device enumeration and all hardware operations fail if the correct driver is absent or architecture-incompatible.

## Client-Facing Interfaces

**WebSocket JSON-RPC-style API:**
- Listener is created in `src/main.rs`; default endpoint is `ws://127.0.0.1:9001`.
- Requests use method names and positional parameter arrays.
- Responses use custom `{error,result,message,id}` objects rather than strict JSON-RPC 2.0 response objects.
- `api/skf_api.js` is the Promise-based browser/Node client wrapper.
- There is no TLS, authentication token, authorization layer, or origin validation.

**Static HTTP Demo:**
- Warp serves `api/` as static files.
- Foreground default is `http://0.0.0.0:8000`; Windows service default is loopback-only.
- `api/skf_api.html` exercises the JavaScript client in `api/skf_api.js`.

## Operating System Integrations

**Windows Service Control Manager:**
- `src/win_service.rs` registers `LiuZXSKFService` as an automatic LocalSystem service.
- Supported commands are `install`, `uninstall`, `start`, `stop`, `status`, and SCM-only `--service`.
- `packaging/windows/install.ps1` copies the package under `%ProgramFiles(x86)%\LiuZX\SKF Service` and configures restart-on-failure with `sc.exe`.
- Service stdout/stderr is redirected to append-only `skf-service.log` beside the executable.

**Windows WoW64 / PE Architecture:**
- `packaging/windows/build.ps1` checks both executable and GM3000 DLL PE Machine values before creating a release.
- Formal Windows release builds use MSVC i686 with static CRT.

## External Commands and Filesystem

**OpenSSL CLI:**
- `IssueCertificate` in `src/main.rs` invokes `openssl req`, `openssl x509`, `openssl ecparam`, and `openssl ec`.
- It creates a demo CA key/certificate and request-specific temporary files under the OS temporary directory.
- This path is explicitly mock/test functionality and is not a production CA integration.

**Local Filesystem:**
- Runtime configuration is read from `config/skf.yaml` or `SKF_CONFIG`.
- The API demo is served from the relative `api/` directory.
- Mock CA state uses shared temporary files such as `skf_ca.key` and `skf_ca.crt`.
- There is no database, remote cache, or persistent application state store.

## Authentication and Secrets

**USB Key PIN:**
- Clients send PIN values over the WebSocket to `CheckPIN`.
- The service stores plaintext PIN strings in the shared `SkfContext.pins` map.
- No external secret manager is integrated.
- `tests/test_all_apis.js` accepts `SKF_PIN`; it has a development fallback value and should not be used with production credentials.

**Transport Authentication:**
- None. Network trust currently depends on binding to loopback and host firewall policy.

## Monitoring and Observability

**Logs:**
- `env_logger` emits application logs to stdout/stderr.
- Windows service mode redirects those streams to `skf-service.log`.
- No log rotation, structured log sink, metrics, tracing, health endpoint, or external error tracker is integrated.

## CI/CD and Release

**GitHub Actions:**
- `.github/workflows/release-windows.yml` supports manual package verification and tag-driven GitHub Releases.
- The workflow runs on `windows-latest`, calls `packaging/windows/build.ps1`, uploads the ZIP as an Actions artifact, and creates a GitHub Release for tags.
- Release permissions use the repository-provided `github.token`; no custom deployment secret is required.
- The workflow validates artifact existence and PE architecture indirectly through the build script, but does not run hardware tests.

**Distribution:**
- Release asset name is `skf-service-windows-x64-gm3000-x86.zip`.
- Portable and installed modes use the same executable and bundled config/API/native DLL payload.

## Environment Matrix

**Development:**
- Rust toolchain, optional Node.js, optional OpenSSL, vendor driver/library, and USB hardware.
- Environment variable names: `SKF_CONFIG`, `SKF_WS_ADDR`, `SKF_HTTP_ADDR`, `RUST_LOG`, `SKF_PIN`, `SKF_WS_URL`, `SKF_NO_RESTART`.

**Production:**
- Windows package has no Rust/Node requirement.
- OpenSSL remains required only if callers use `IssueCertificate`.
- Availability depends on the local vendor driver and physical token; there is no remote failover.

## Webhooks and Remote SaaS

- No incoming or outgoing webhooks.
- No database, cloud API, identity provider, payment service, analytics service, or remote PKI endpoint is integrated.

---

*Integration audit: 2026-09-11*
*Update when adding providers, transports, observability, or deployment targets*

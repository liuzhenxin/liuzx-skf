# SKF Service (Rust)

A Rust-based SKF interface service that provides a unified calling interface via WebSocket, supporting dynamic switching between multiple SKF vendors (e.g., GM3000, 3000GM).

## Features

- **Unified WebSocket / HTTP API**: Access hardware security modules (HSM) / USB keys through a simple JSON-RPC interface.
- **Multi-Vendor Support**: Dynamically load different SKF libraries based on configuration or runtime selection.
- **Cross-Platform**: Configurable for Windows, Linux, and macOS (driver dependent).

## Configuration (`config/skf.yaml`)

The service is configured via `config/skf.yaml`.

- **default**: The default provider alias to use if none is specified.
- **vendor**: Maps USB Device VendorID:ProductID (VID:PID) to a provider alias.
- **`<ProviderAlias>`**: Defines the library path for a specific provider on different OSs.

Example:

```yaml
default: "GM3000"

vendor:
  "055c:e618": "GM3000"

GM3000:
  windows: "native\\GM3000\\windows\\mtoken_gm3000.dll"
  linux:  "native/GM3000/linux/libgm3000.1.0.so"
  macos: "native/GM3000/macos/x86_64/libgm3000.1.0.dylib"
```

## Running

```bash
cargo run
```

The service will start listening on `ws://127.0.0.1:9001` for WebSocket and `http://0.0.0.0:8000` for HTTP API Demo.

## Standalone Windows x64 Package (GM3000)

The bundled `native/GM3000/windows/mtoken_gm3000.dll` is a PE32/i386
(32-bit) DLL. Windows x64 can run an x86 service through WoW64, but an x64
process cannot load this DLL. The required deployment combination is:

| Component | Architecture |
| --- | --- |
| Windows | x86_64 |
| `skf-service.exe` | x86 / i686 (`Machine=0x014C`) |
| `mtoken_gm3000.dll` | x86 / i386 (`Machine=0x014C`) |

Build and assemble the standalone package on Windows:

```powershell
cd D:\liuzx-skf
powershell -ExecutionPolicy Bypass -File .\packaging\windows\build.ps1
```

Output:

```text
dist\skf-service-windows-x64-gm3000-x86\
dist\skf-service-windows-x64-gm3000-x86.zip
```

After extraction:

- Run `run.bat` for portable foreground mode.
- Run `install.bat` as Administrator to install and immediately start the
  `LiuZXSKFService` Windows service with automatic startup.
- Run `uninstall.bat` as Administrator to stop and remove the service.

The runtime config must use the packaged DLL:

```yaml
GM3000:
  windows: "native\\GM3000\\windows\\mtoken_gm3000.dll"
```

The target machine does not need Rust or Node.js, but it still requires the
GM3000 hardware driver. See
[`packaging/windows/README-Windows-x86_64.md`](packaging/windows/README-Windows-x86_64.md)
for packaging, PE architecture verification, service management and
`LoadLibraryExW failed` troubleshooting.

## WebSocket API

The service uses JSON-RPC 2.0 style messages.

### Endpoint

`ws://127.0.0.1:9001`

### Methods

All parameters are passed as arrays in the `params` field.

- **`SetLanguage`**: Set the language for error messages.
  - Params: `["CN"]` or `["EN"]`
- **`EnumProvider`**: List supported providers or query a specific device's provider.
  - Params: `[]` (list all) or `["VID:PID"]` (query specific)
- **`EnumDevice`**: List devices for a specific provider.
  - Params: `["ProviderAlias"]`
- **`WaitForDevEvent`**: Block and wait for a device event (plug/unplug).
  - Params: `["ProviderAlias"]`
  - Response: `{ deviceName: string, event: number }`
- **`EnumApplication`**: Enumerate applications on a specific device.
  - Params: `["ProviderAlias", "DeviceName"]`
- **`EnumContainer`**: Enumerate containers in a specific application.
  - Params: `["ProviderAlias", "DeviceName", "AppName"]`
- **`DeleteContainer`**: Delete a container in a specific application.
  - Params: `["ProviderAlias", "DeviceName", "AppName", "ContainerName", "PIN"]`
- **`CheckPIN`**: Verify user PIN and cache it (subsequent operations won't require re-verification).
  - Params: `["CertKeyPath", "PIN"]`
- **`CreateContainer`**: Create a new container.
  - Params: `["ProviderAlias", "DeviceName", "AppName", "ContainerName"]`
- **`ConnectDev`**: Connect to a specific device.
  - Params: `["DeviceName"]`
  - Response: Device Handle (integer as string)
- **`DisConnectDev`**: Disconnect a device.
  - Params: `[DeviceHandle]`
- **`FindCertificates`**: Search for certificates across all connected devices and containers.
  - Params: `["Filter"]` (Optional: `"Sign"`, `"Enc"`, or empty for both)
  - Response: Array of certificate objects (`{key, value, type, cert}`)
- **`SignData`**: Sign data using the specified certificate.
  - Params: `["CertKeyPath", "DataBase64", "PIN"]`
- **`Digest`**: Compute a hash digest using the device.
  - Params: `["ProviderAlias", "DeviceName", "DataBase64", "Alg"]` (Alg: "SM3", "SHA1", "SHA256")
- **`CreatePKCS10`**: Create a PKCS#10 Certificate Signing Request.
  - Params: `["ProviderAlias", "DeviceName", "AppName", "PIN", "SubjectDN", "KeyType", KeyLength]`
- **`IssueCertificate`**: Issue a certificate from a CSR (for testing / mocking).
  - Params: `["CSR", DoubleBool]`
- **`ImportCertificate`**: Import a certificate into a container.
  - Params: `["ProviderAlias", "DeviceName", "AppName", "ContainerName", "PIN", IsSignCertBool, "CertData"]`
- **`ImportKeyPair`**: Import an encryption key pair into a container.
  - Params: `["ProviderAlias", "DeviceName", "AppName", "ContainerName", "Alg", "EncKeyPairBase64", "WrapKeyBase64", "SM4Mode?"]`
  - `Alg`: "SM2"/"ECC" or "RSA"
  - `SM4Mode`: SM4 encryption mode for RSA key import ("ECB" or "CBC", default ECB)
- **`EncryptData`**: Encrypt data using SM4-CBC algorithm.
  - Params: `["CertKeyPath", "DataBase64", "IVBase64", "PaddingType", "SymKeyBase64?"]`
  - `PaddingType`: 1=PKCS#7 padding (default), 0=no padding
  - `SymKeyBase64`: Optional external symmetric key (16 bytes), uses container key if not provided
- **`DecryptData`**: Decrypt data using SM4-CBC algorithm.
  - Params: `["CertKeyPath", "EncryptedDataBase64", "IVBase64", "PaddingType", "SymKeyBase64?"]`
- **`GenerateRandom`**: Generate random data using the device.
  - Params: `[DeviceHandle, Length]`

## Example Usage

Connect to the WebSocket and send:

```json
{
  "jsonrpc": "2.0",
  "method": "EnumDevice",
  "params": ["GM3000"],
  "id": 1
}
```

## Development, module layout, and the gate

Rust modules after the refactor (see `src/lib.rs`):

| Module | Responsibility |
| ------ | -------------- |
| `protocol/` | `RpcRequest`/`RpcResponse`/`Language` and typed parameter access |
| `domain/` | security-relevant handlers, read-only/destructive classification, restricted set |
| `session/` | session, authorization (TTL/deadline), opaque handle table |
| `provider/` | SKF provider abstraction (native guards + failure-injecting fake) |
| `service_state/` | Windows service startup status file and exit codes |
| `logging/` | structured JSON-line rotating logs and redaction |
| `diagnostic/` | local diagnostic report |
| `server/` | transport, bind/serve, session seam |
| `config/` `crypto/` `skf/` | configuration, pure encoding logic, SKF C ABI adapter |

Common commands:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo check --target i686-pc-windows-gnu
skf-service.exe diagnose --json        # local diagnostics (no paths/credentials)
```

See `docs/CI.md` for the merge gate, `THREAT-MODEL.md` for the trust boundary,
`docs/SESSION-AND-LIMITS.md` for the session/limits model, and
`docs/WINDOWS-SERVICE.md` for the Windows service.

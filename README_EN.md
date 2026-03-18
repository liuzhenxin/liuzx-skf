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
  windows: "%ProgramFiles(X86)%\\GM3000\\mtoken_gm3000.dll"
  linux:  "native/GM3000/linux/libgm3000.1.0.so"
  macos: "native/GM3000/macos/x86_64/libgm3000.1.0.dylib"
```

## Running

```bash
cargo run
```

The service will start listening on `ws://127.0.0.1:9001` for WebSocket and `http://0.0.0.0:8000` for HTTP API Demo.

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
  - Params: `["ProviderAlias", "DeviceName", "AppName", "ContainerName", "PIN", "Alg", "EncKeyPairBase64", "WrapKeyBase64"]`
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

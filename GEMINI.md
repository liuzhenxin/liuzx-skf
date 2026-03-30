# GEMINI.md - SKF Service

Instructional context for the SKF Service project.

## Project Overview

**SKF Service** is a Rust-based application that acts as a bridge between high-level applications (via WebSocket/JSON-RPC) and low-level native SKF (Smart Key Function) libraries provided by various security hardware vendors (e.g., Longmai GM3000, Feitian 3000GM).

### Key Technologies
- **Rust**: Core logic, high-performance concurrency, and safe memory management.
- **WebSocket (tokio-tungstenite)**: Provides a bi-directional JSON-RPC interface for clients to call SKF functions.
- **HTTP (warp)**: Serves a built-in API demo page.
- **libloading**: Dynamically loads native `.dll`, `.so`, or `.dylib` libraries at runtime.
- **smcrypto / x509-parser**: Handles cryptographic operations and X.509 certificate parsing.

### Architecture
- `src/main.rs`: Entry point. Manages the WebSocket server, JSON-RPC request routing, and the static file server for the UI.
- `src/skf/`:
    - `mod.rs`: Module definition.
    - `api.rs`: Abstract wrapper around native SKF function pointers.
    - `types.rs`: C-compatible structures and constants mapping the SKF standard.
- `api/`: Front-end assets (`skf_api.html`, `skf_api.js`) for the demo dashboard.
- `config/skf.yaml`: Maps hardware vendors (VID:PID) to their respective native library paths across different operating systems.
- `native/`: Repository for vendor-supplied binary libraries.

## Building and Running

### Build
To compile the project:
```bash
cargo build
```

### Run
To start the WebSocket and HTTP services:
```bash
cargo run
```
- **WebSocket URL**: `ws://127.0.0.1:9001`
- **HTTP Demo**: `http://localhost:8000/skf_api.html`

### Testing
To run the automated integration tests:
```bash
./tests/run_tests.sh
```
*Note: Requires Node.js and `npm install` for test script dependencies.*

## Development Conventions

### Coding Style
- Follow standard Rust idioms where possible.
- **Native Interop**: Structs and constants in `src/skf/types.rs` use uppercase names (e.g., `ULONG`, `DEVHANDLE`) to match the original C SKF specifications for clarity during interop debugging.
- **RPC Protocol**: Communication follows a JSON-RPC-like structure with `method`, `params`, and `id` fields. Responses include an `error` code (0 for success) and a `result` payload.

### Library Paths
- Paths in `config/skf.yaml` can use environment variables (e.g., `%ProgramFiles(X86)%`).
- The service dynamically determines which library to load based on the `provider` parameter in RPC calls.

### UI Modifications
- The demo page in `api/skf_api.html` uses a modern light-themed UI with CSS variables. 
- When updating the UI, ensure compatibility with the `SKFClient` class defined in `api/skf_api.js`.

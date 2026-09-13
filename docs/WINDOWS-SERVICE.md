# Windows Service

`skf-service.exe` can run as a Windows service (`LiuZXSKFService`) managed by the
Service Control Manager. This document describes how it reports startup, how to
install/upgrade/uninstall it, and how to verify the install on Windows.

## Startup phases

The service writes `service-state.json` next to the executable as it initializes.
`skf-service.exe status` reads it, so an operator can distinguish "the process is
in the SCM" from "the service is usable".

| stage | meaning |
|-------|---------|
| `config` | reading and parsing `config/skf.yaml` |
| `provider` | resolving the configured provider for this OS |
| `bind` | binding the WebSocket listener |
| `serve` | listening and serving requests |

### Fields

| field | meaning |
|-------|---------|
| `state` | `start_pending` \| `running` \| `failed` \| `stopped` |
| `stage` | the phase above |
| `code` | the service exit code on failure |
| `reason` | a short failure reason (never a config value or path) |
| `pid` | the process id |
| `address` | the bound WebSocket address when running |
| `updated` | Unix epoch seconds of the last write |

The file is written atomically and must never contain configuration values, library
paths, PINs, or key material. It is advisory: if it is missing, `status` falls back
to SCM output.

## Exit codes

On a startup failure the service reports `Stopped` with a **service-specific** exit
code and writes a `failed` status file:

| code | phase | meaning |
|------|-------|---------|
| `1` | `config` | configuration could not be read or parsed |
| `2` | `provider` | the default provider has no path for this OS |
| `3` | `bind` | the WebSocket listener could not be bound, **or** a non-loopback bind was refused because the exposure rule was not satisfied |
| `4` | `serve` | a runtime error while serving |

The non-zero code makes the restart-on-failure action configured by the installer
(`sc failure LiuZXSKFService reset= 86400 actions= restart/5000/restart/10000/restart/60000`)
actually fire.

## TLS and client authentication

TLS is optional. Without a `tls:` block the listener is plaintext loopback, exactly
as before v0.4.0. To serve TLS, point the service at a PEM certificate chain and
private key:

```yaml
default: "GM3000"
vendor:
  "055c:e618": "GM3000"
tls:
  cert_file: "config/server.crt"
  key_file: "config/server.key"
  client_auth: "none"        # none | mtls | token
GM3000:
  windows: "native\\GM3000\\windows\\mtoken_gm3000.dll"
```

Client authentication (`tls.client_auth`):

- `none` — no client authentication. Accepted **only** on a loopback bind.
- `mtls` — a client certificate is required and verified against `client_ca_file`
  (a PEM CA bundle). A missing or untrusted certificate fails the TLS handshake.
- `token` — the WebSocket upgrade must carry `Authorization: Bearer <token>`; a
  missing or wrong token is rejected with `401` before a session exists.

A non-loopback bind requires **all** of: `allow_remote: true` (or
`SKF_ALLOW_REMOTE=1`), TLS (`cert_file` + `key_file`), and client authentication.
Otherwise the service stops at the `bind` stage with exit code `3` and a message
naming the missing controls.

### Secret handling

The private key and the bearer token are secrets:

- Never write the token into the YAML. Supply it through `SKF_TLS_TOKEN` or a
  `token_file` readable only by the service account.
- The token and the private key never appear in logs, `diagnose` output, the status
  file, or release metadata.
- `skf-service.exe diagnose --json` reports only `tls_enabled` and
  `tls_cert_loaded` booleans (no paths, no key material).

Environment overrides: `SKF_TLS_CERT`, `SKF_TLS_KEY`, `SKF_TLS_CLIENT_AUTH`,
`SKF_TLS_CLIENT_CA`, `SKF_TLS_TOKEN`, `SKF_TLS_TOKEN_FILE`.

## Provider resolution: fatal vs degraded

- **Fatal (non-zero exit):** the YAML is unparseable, the `default` alias is not
  defined, or the default alias has **no path for the current OS**.
- **Degraded (still `Running`):** a path is configured but the library file is
  missing or fails to load. The provider factory substitutes `UnavailableProvider`,
  and the service answers every request with the established `Load Lib Failed`
  error instead of refusing to start.

The degraded path is deliberate: a machine without the vendor DLL (for example a
build or CI host) must still be able to start the service. Only a configuration
mistake is fatal.

## Install

Run `install.ps1` (or right-click `install.bat`) **as administrator**.

- Copies the payload to `%ProgramFiles(x86)%\LiuZX\SKF Service`.
- Applies a restrictive ACL to that directory:
  - `SYSTEM:(OI)(CI)F`
  - `Administrators:(OI)(CI)F`
  - `Users:(OI)(CI)RX`

  so a non-administrator cannot replace the loaded `skf-service.exe` or
  `mtoken_gm3000.dll`. The installer verifies the ACL and fails if `Users` still
  has write, modify, or full control.
- Registers the service as `LocalSystem`, `AutoStart`, and starts it immediately.
- Configures restart-on-failure.

The service account stays `LocalSystem` because the GM3000 driver must be
accessible; the ACL restricts files, not the account.

## Upgrade over a running installation

Re-run `install.ps1`. It stops the service, waits until the service is `Stopped`
**and** the executable can be opened exclusively (up to 30 s), then replaces the
payload and restarts. It refuses to continue on timeout rather than copying over
locked files.

## Uninstall

Run `uninstall.ps1` (or `uninstall.bat`) **as administrator**. It stops and deletes
the registration, waits for the process to exit and release its files, then removes
the install directory. A cleanup failure is reported, not swallowed.

## `status` examples

```
Startup: Running (stage=serve, address=127.0.0.1:9001)
Service 'LiuZXSKFService': Running (pid=1234)
Executable: C:\Program Files (x86)\LiuZX\SKF Service\skf-service.exe
```

```
Startup: Failed (stage=bind, code=3)
Reason: bind error: failed to bind WebSocket listener on 127.0.0.1:9001: ...
Service 'LiuZXSKFService': Stopped (pid=0)
```

A refused remote bind uses the same stage and code with a different reason:

```
Startup: Failed (stage=bind, code=3)
Reason: bind error: refusing to bind WebSocket listener to non-loopback address 0.0.0.0:9001; remote binding requires all of: ...
Service 'LiuZXSKFService': Stopped (pid=0)
```

## Verifying the install

On a Windows host as administrator:

```powershell
powershell -ExecutionPolicy Bypass -File packaging\windows\verify-service.ps1
```

This runs install → status(`stage=serve`) → upgrade over running → ACL check →
uninstall (asserts no leftover registration or directory) → reinstall → uninstall,
and prints `ALL CHECKS PASSED` on success. Use `-KeepInstalled` to leave the final
install in place while you watch `sc query LiuZXSKFService` during startup.

## Logs

stdout/stderr are redirected to `%ProgramFiles(x86)%\LiuZX\SKF Service\skf-service.log`
when running as a service.

# v0.4.0 Research — Pitfalls

1. **rustls crypto-provider default.** `rustls 0.23` defaults to `aws-lc-rs`, whose C
   build is fragile on `i686-pc-windows-gnu`. **Pin the `ring` provider** on both
   `rustls` and `tokio-rustls`, verified by `cargo check --target i686-pc-windows-gnu`.

2. **Handshake failure must not kill the accept loop.** A malformed ClientHello or a
   rejected client certificate must be logged as a class and the connection dropped;
   `serve` keeps accepting. Mirror the existing `spawn_client` error handling.

3. **`accept_hdr_async` for the bearer token.** `accept_async` cannot see HTTP
   headers. Use `accept_hdr_async_with_config` so (a) the `Authorization` header is
   readable and (b) the 1 MiB frame/message limits from phase 3 are preserved.

4. **`TlsStream` is not `TcpStream`.** `spawn_client` must be generic over
   `AsyncRead + AsyncWrite + Unpin + Send` (or take a boxed stream) so the same loop
   serves plaintext and TLS. Do not fork the loop.

5. **Sensitive material never leaves its boundary.** The private key, client CA
   material, and bearer token are read from files/env and must not appear in logs,
   `diagnose` output, the status file, or release metadata. `tests/log_redaction.rs`
   covers log macros; extend `diagnostic.rs` to report booleans only.

6. **`SkfConfig` flatten.** Add `tls` as a **named** field; an unnamed nested key is
   captured by the flattened provider map and breaks parsing (phase 2 D-07). Test
   that a config with `tls:` still parses and providers still load.

7. **Bind-policy interaction.** `bind_with_progress` currently gates on
   `allows_remote()`. The new rule (remote requires remote-opt-in **and** TLS **and**
   client auth) must not change the loopback default nor the 37 fixtures. Update
   `tests/loopback_gate.rs` and keep its refusal message explicit.

8. **Backward compatibility is the default.** No TLS by default; loopback plaintext
   keeps working, v0.2.0 clients unchanged. TLS is strictly opt-in.

9. **mTLS identity without leaking the certificate.** Extract the subject CN for
   audit only; never log the peer certificate or its serial. Prefer
   `WebPkiClientVerifier` over a hand-rolled verifier.

10. **Test certificates must be hermetic.** Do not depend on system OpenSSL text
    (the `IssueCertificate` lesson: its recorded error drifted with the OpenSSL
    version). Generate test certs with **`rcgen` as a dev-dependency** (test-only,
    not shipped) or, failing that, a checked-in test cert/key pair with a far-future
    expiry — never parse OpenSSL CLI output in assertions.

11. **Connection cap vs handshake.** The `Semaphore` permit must be acquired before
    the TLS handshake and held for the connection's lifetime; a failed handshake must
    release it. Otherwise a TLS handshake flood can exceed the cap.

12. **Audit volume.** Log authorization *decisions* (grant / deny / expiry /
    device-unavailable), not every request. Keep fields fixed and non-sensitive so the
    JSON logger and the redaction scan stay simple.

13. **`diagnose` and status.** `diagnose` must reflect TLS/client-auth booleans
    without reading secrets; the service status file stays free of TLS material.

# Operation Classification

Every JSON-RPC method the service exposes is classified as **ReadOnly** or
**Destructive**.

The source of truth is `src/domain/classification.rs`
(`CLASSIFIED_METHODS`). This table is a rendering of it; if the two disagree, the
code wins. `tests/classification_invariants.rs` asserts that the classification
covers exactly the methods in `tests/common/mod.rs::EXPECTED_METHODS`, so a new
method cannot be added without a class.

## Classification rule

An operation is **Destructive** when it can:

- change persisted device or token state;
- consume a PIN retry;
- create, import, or move key material;
- open or close a **device, application, or container** resource;
- change a lock.

Otherwise it is **ReadOnly**. Stateless cryptographic operations
(`SignData`, `EncryptData`, `GenerateRandom`) are ReadOnly: they do not change
persisted state. Streaming digest state (`DigestInit`/`DigestUpdate`/
`DigestFinal`/`CloseHash`) is session-scoped in-memory state that is released
with the session and does not change device or token state, so it is ReadOnly.

> **This phase classifies and documents only.** Nothing consults the class on a
> request path — no authorization, audit or confirmation is derived from it yet.
> Doing so is a later phase's concern and would otherwise change the frozen
> v0.2.0 behaviour.

## Methods

| Method | Class | Why |
|--------|-------|-----|
| SetLanguage | ReadOnly | Changes only per-connection display state |
| WaitForDevEvent | ReadOnly | Waits for a device event; changes nothing |
| EnumProvider | ReadOnly | Lists configured providers |
| EnumDevice | ReadOnly | Lists devices |
| ConnectDev | Destructive | Opens a native device resource |
| EnumApplication | ReadOnly | Lists applications |
| EnumContainer | ReadOnly | Lists containers |
| DeleteContainer | Destructive | Removes persisted container data |
| IssueCertificate | Destructive | Generates key material and a certificate |
| ImportCertificate | Destructive | Writes a certificate into the token |
| SignData | ReadOnly | Signs data; no persisted state changes |
| DisConnectDev | Destructive | Closes a native device resource |
| FindCertificates | ReadOnly | Enumerates and reads certificates |
| GenerateRandom | ReadOnly | Stateless random generation |
| Digest | ReadOnly | Stateless hash computation |
| CheckPIN | Destructive | Consumes a PIN retry and grants session authorization |
| CreatePKCS10 | Destructive | Generates a key pair and a container |
| EncryptData | ReadOnly | Encrypts supplied data; no persisted state changes |
| DecryptData | ReadOnly | Decrypts supplied data; no persisted state changes |
| GetDevInfo | ReadOnly | Reads device metadata |
| GetDevState | ReadOnly | Reads device state |
| SetLabel | Destructive | Changes persisted device label |
| ECCVerify | ReadOnly | Verifies a signature |
| CreateContainer | Destructive | Creates persisted container data |
| GetContainerType | ReadOnly | Reads container type |
| RSASignData | ReadOnly | Signs data; no persisted state changes |
| LockDev | Destructive | Changes device lock state |
| UnlockDev | Destructive | Changes device lock state |
| Transmit | Destructive | Sends arbitrary command data to the token |
| CancelWaitForDevEvent | ReadOnly | Cancels a pending wait |
| GenECCKeyPair | Destructive | Generates and stores an ECC key pair |
| GenRSAKeyPair | Destructive | Generates and stores an RSA key pair |
| RSAVerify | ReadOnly | Verifies a signature |
| DigestInit | ReadOnly | Starts a streaming hash; no persisted state |
| DigestUpdate | ReadOnly | Feeds a streaming hash |
| DigestFinal | ReadOnly | Finalizes a streaming hash |
| CloseHash | ReadOnly | Releases an in-memory hash context |

## Counts

- Destructive: 14
- ReadOnly: 23
- Total: 37

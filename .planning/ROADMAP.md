# Roadmap: LiuZX SKF Service

## Milestones

- ✅ **v0.3.0 Production Hardening** — Phases 1-6 (shipped 2026-09-12; patch v0.3.1)
- 🚧 **v0.4.0 Secure Remote Operation** — Phases 7-10 (in progress)

## Phases

<details>
<summary>✅ v0.3.0 Production Hardening (Phases 1-6) — SHIPPED 2026-09-12</summary>

- [x] Phase 1: Structural Foundation and Test Seam (4/4 plans)
- [x] Phase 2: Session Authorization and Resource Ownership (4/4 plans)
- [x] Phase 3: Transport Hardening and Concurrency (4/4 plans)
- [x] Phase 4: Format/Lint Normalization and CI Gate (3/3 plans)
- [x] Phase 5: Windows Service Reliability (2/2 plans)
- [x] Phase 6: Observability, Diagnostics, and Release Verification (3/3 plans)

Full detail: `.planning/milestones/v0.3.0-ROADMAP.md`; patch v0.3.1 fixed
device-removal grant clearing.

</details>

### 🚧 v0.4.0 Secure Remote Operation (In Progress)

- [x] Phase 7: TLS Termination (TLS-01..04, COMPAT-01) — 2/2 plans
- [x] Phase 8: Client Authentication (AUTH-01..04) — 3/3 plans
- [x] Phase 9: Bind Policy and Authorization Audit (BIND-01..03, AUD-01..02) — 3/3 plans
- [ ] Phase 10: Documentation and Quality Closeout (COMPAT-02, QA-01, QA-02)

## Phase Details (v0.4.0)

### Phase 7: TLS Termination
**Goal**: 让 WebSocket 监听器可以选择性地以 TLS 提供服务，且默认行为与 v0.2.0 明文回环完全一致。
**Depends on**: Phase 3 (transport) and Phase 5 (startup classification)
**Requirements**: [TLS-01, TLS-02, TLS-03, TLS-04, COMPAT-01]
**Success Criteria** (what must be TRUE):
  1. With a configured certificate/key, a TLS client completes the handshake and invokes a method successfully; with no TLS config the listener stays plaintext.
  2. A missing/unreadable certificate or key fails startup with a distinct exit code, and no key material appears in the error output.
  3. The private key never appears in logs, `diagnose`, the status file, or release metadata.
  4. The 37 frozen fixtures still replay (loopback plaintext is the default), and `cargo check --target i686-pc-windows-gnu` stays green with the `ring` provider.
**Plans**: 2 plans

Plans:
- [x] 07-01: Optional TLS termination for the WebSocket listener
- [x] 07-02: Secret safety and diagnostics for TLS

### Phase 8: Client Authentication
**Goal**: 未认证连接不能调用任何方法；支持 mTLS 客户端证书与 bearer 令牌两种模式。
**Depends on**: Phase 7
**Requirements**: [AUTH-01, AUTH-02, AUTH-03, AUTH-04]
**Success Criteria** (what must be TRUE):
  1. With `client_auth: mtls`, a connection without a client certificate is refused, a valid one works, and an invalid/unknown one is refused.
  2. With `client_auth: token`, a missing or incorrect bearer token is refused; the correct token works.
  3. `client_auth: none` is accepted only on a loopback bind.
  4. The authenticated identity (mTLS CN or `token`) is available for audit, and neither the token nor the client certificate is logged.
**Plans**: 3 plans

Plans:
- [x] 08-01: Client-auth configuration, modes, and identity types
- [x] 08-02: Server-side mTLS and bearer-token enforcement
- [x] 08-03: Bind coupling and hermetic client-auth tests

### Phase 9: Bind Policy and Authorization Audit
**Goal**: 非回环暴露只有在真正的传输安全与访问控制齐备时才被允许，并留下非敏感的授权决策审计。
**Depends on**: Phase 8
**Requirements**: [BIND-01, BIND-02, BIND-03, AUD-01, AUD-02]
**Success Criteria** (what must be TRUE):
  1. A non-loopback bind is refused unless remote opt-in AND TLS AND client authentication are all enabled; it is allowed when all three are.
  2. The refusal happens before binding, names both requirements, and exits with the bind startup code.
  3. Loopback defaults are unchanged and `loopback_gate` tests reflect the new rule.
  4. Authorization decisions (grant/deny/expiry/device-unavailable) are logged as structured events with no PIN, key, payload, or token, and the redaction scan passes.
**Plans**: 3 plans

Plans:
- [x] 09-01: Structured authorization audit
- [x] 09-02: Combined exposure rule for non-loopback binding
- [x] 09-03: Audit end-to-end proof and regression

### Phase 10: Documentation and Quality Closeout
**Goal**: 记录新的信任边界与迁移路径，并补齐上一里程碑遗留的质量项。
**Depends on**: Phase 9
**Requirements**: [COMPAT-02, QA-01, QA-02]
**Success Criteria** (what must be TRUE):
  1. `THREAT-MODEL.md`, `docs/SESSION-AND-LIMITS.md`, `RELEASE-NOTES.md`, and `docs/WINDOWS-SERVICE.md` describe TLS, client authentication, the tightened bind policy, and how to migrate.
  2. Nyquist sign-off for phases 3-6 is recorded, and the Linux CI fast-job test list includes the phase 5/6 suites.
  3. TLS/auth tests run with hermetic certificates (no token) and the i686 cross-compile stays green.
**Plans**: 3 plans

Plans:
- [ ] 10-01: Document TLS, client authentication, and the bind policy
- [ ] 10-02: CI fast-job coverage and Nyquist sign-off
- [ ] 10-03: Hermetic proof and milestone-level regression

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1-6 | v0.3.0 | 20/20 | Complete | 2026-09-12 |
| 7. TLS Termination | v0.4.0 | 2/2 | Complete | 2026-09-12 |
| 8. Client Authentication | v0.4.0 | 3/3 | Complete | 2026-09-12 |
| 9. Bind Policy and Audit | v0.4.0 | 3/3 | Complete | 2026-09-12 |
| 10. Docs and Quality | v0.4.0 | 0/0 | Not started | - |

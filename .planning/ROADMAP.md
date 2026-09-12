# Roadmap: LiuZX SKF Service

## Overview

v0.3.0 将当前可运行但未加固的服务转变为可验证、可运维的生产基线。旅程从"让代码第一次可被测试"开始，因为安全重构在零测试基线上无法验证；随后依次建立 provider 边界、会话授权与资源归属、传输与并发约束、可执行的质量门禁、可靠的 Windows 服务生命周期，最后以可观测性和发布完整性收尾。整个过程保持 v0.2.0 客户端契约与 GM3000 i686 部署组合不变。

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

- [x] **Phase 1: Structural Foundation and Test Seam** - 拆分为 library crate，固化 v0.2.0 契约夹具，建立 provider 抽象与可注入失败的 fake (completed 2026-09-11)
- [ ] **Phase 2: Session Authorization and Resource Ownership** - 以会话隔离授权，用不透明标识替代原生句柄，确定性释放资源
- [ ] **Phase 3: Transport Hardening and Concurrency** - 建立帧/载荷/连接/时长边界，阻塞 FFI 移出异步 worker，按设备串行化
- [ ] **Phase 4: Format/Lint Normalization and CI Gate** - 归一化格式与 lint，启用会真正阻断合并的 CI 门禁
- [ ] **Phase 5: Windows Service Reliability** - 准确的就绪状态与失败退出码，安装目录权限加固，完整生命周期验证
- [ ] **Phase 6: Observability, Diagnostics, and Release Verification** - 结构化轮转日志、脱敏诊断、协议版本协商、发布完整性与威胁模型

## Phase Details

### Phase 1: Structural Foundation and Test Seam
**Goal**: 让代码库首次可被自动化验证：逻辑可单测、协议契约被夹具冻结、SKF 调用隔离在可替换的 provider 边界之后。
**Depends on**: Nothing (first phase)
**Requirements**: [FOUND-01, FOUND-02, FOUND-03, FOUND-04, FOUND-05, FOUND-06]
**Success Criteria** (what must be TRUE):
  1. Developer runs `cargo test` with no USB Key, no vendor driver, and no network, and a non-zero number of tests executes and passes
  2. Every currently supported method has a frozen v0.2.0 request/response fixture, and the suite fails if any shape changes
  3. All SKF operations are reached through a provider abstraction whose fake implementation can be configured to inject wrong PIN, missing container, missing symbol, device removal, and a blocking call
  4. Pure encoding logic (DER, subject DN, SPKI, base64/hex) has tests that run without hardware or OpenSSL
  5. The Windows i686 build still produces `skf-service.exe` with `Machine=0x014C` after the split
**Plans**: 4 plans

Plans:
- [x] 01-01: 录制并冻结 v0.2.0 契约夹具（重构前完成，含 oracle 自检）
- [x] 01-02: library crate 拆分与纯逻辑提取（crypto/、config/ 单测）
- [x] 01-03: SKF provider 抽象与可注入失败的 fake（含不变量测试）
- [x] 01-04: provider 注入、临时端口服务器、端到端契约重放与 Windows 构建验证

### Phase 2: Session Authorization and Resource Ownership
**Goal**: 消除共享凭据与客户端可控原生句柄两个最严重缺陷：授权按会话隔离并自动失效，所有原生资源有明确归属和确定性释放。
**Depends on**: Phase 1
**Requirements**: [SESS-01, SESS-02, SESS-03, SESS-04, SESS-05, SESS-06, RES-01, RES-02, RES-03, RES-04, RES-05]
**Success Criteria** (what must be TRUE):
  1. A test demonstrates that a PIN verified by one session does not authorize a second concurrent session
  2. Authorization expires after the configured interval and is cleared on disconnect and on device removal
  3. After PIN verification no persistent structure retains a recoverable copy of the PIN value
  4. Clients reference devices, applications, containers, and streaming crypto objects only through opaque identifiers, and an unknown, expired, or wrong-kind identifier is rejected
  5. Native resources are released on disconnect (including abrupt disconnect), on native error paths, and when a session ends; streaming digest state cannot be observed or reused by another session
**Plans**: 4 plans

Plans:
- [x] 02-01: 会话骨架与注册表（ID 生成、`Weak` marker、按值持有状态）
- [x] 02-02: 每会话授权与 TTL（环境变量读取、三条清除路径、PIN 不保留）
- [x] 02-03: 不透明句柄（会话级句柄表、拒绝语义、V-1/V-2 回归）
- [x] 02-04: 协议层抽取 + 12 分支迁移至 domain/ + 错误路径释放验证

### Phase 3: Transport Hardening and Concurrency
**Goal**: 让服务在异常输入、缓慢设备与并发客户端下保持有界行为，并把厂商 DLL 的阻塞与线程安全风险限制在受控边界内。
**Depends on**: Phase 2
**Requirements**: [TRANS-01, TRANS-02, TRANS-03, TRANS-04, TRANS-05, TRANS-06, TRANS-07]
**Success Criteria** (what must be TRUE):
  1. A frame or payload above the documented limit is rejected before large allocation occurs, and the connection count is bounded
  2. Blocking vendor calls no longer execute on async runtime worker threads, and one stalled token operation does not stall unrelated clients
  3. A test fails if two operations use the same device handle concurrently, and no more than one documented `unsafe` Send/Sync assertion exists in the crate
  4. The service refuses to bind a non-loopback address unless configuration explicitly opts in
  5. Destructive operations are distinguishable from read-only ones through a documented classification
**Plans**: TBD

Plans:
- [x] 03-01: TBD
- [x] 03-02: TBD

### Phase 4: Format/Lint Normalization and CI Gate
**Goal**: 建立真实生效的合并门禁：格式与 lint 基线干净，CI 运行硬件无关测试并在失败时阻断合并。
**Depends on**: Phase 3
**Requirements**: [QUAL-01, QUAL-02, QUAL-03, QUAL-04]
**Success Criteria** (what must be TRUE):
  1. `cargo fmt -- --check` passes repository-wide after an isolated formatting-only change
  2. `cargo clippy --all-targets` reports no actionable warnings, and every surviving allow is explicitly scoped and documented
  3. CI runs formatting, linting, hardware-free tests, and an i686 Windows compile check on pull requests and main-branch pushes
  4. A deliberately failing test blocks the CI check, proving the gate is enforced rather than advisory
**Plans**: TBD

Plans:
- [x] 04-01: TBD
- [x] 04-02: TBD

### Phase 5: Windows Service Reliability
**Goal**: 让 Windows 服务如实反映自身可用性，并让安装、升级、卸载在权限与清理上可靠。
**Depends on**: Phase 3 (independent of Phase 4; may execute in parallel)
**Requirements**: [SVC-01, SVC-02, SVC-03, SVC-04, SVC-05]
**Success Criteria** (what must be TRUE):
  1. The SCM observes StartPending during initialization and Running only after the listener is bound and the configured provider is resolved
  2. Starting with a deliberately invalid configuration exits with a distinct non-zero service exit code and the configured restart-on-failure action fires
  3. The install directory permissions prevent a non-administrator from replacing the loaded executable or DLL
  4. Install, run, upgrade over a running installation, and uninstall leave no stale service registration or locked files, and reinstall succeeds
  5. `status` reports the failing or completed startup stage so an operator can distinguish "process running" from "service usable"
**Plans**: TBD

Plans:
- [x] 05-01: TBD
- [ ] 05-02: TBD

### Phase 6: Observability, Diagnostics, and Release Verification
**Goal**: 让运维人员能够诊断问题而不泄露敏感信息，并让发布产物可被独立校验。
**Depends on**: Phase 5
**Requirements**: [OBS-01, OBS-02, OBS-03, OBS-04, REL-01, REL-02, REL-03, REL-04, REL-05, DOC-01, DOC-02, DOC-03]
**Success Criteria** (what must be TRUE):
  1. Service logs are structured, leveled, written to a rotating file with bounded retention, and proven to contain no PIN, private key, session key, or decrypted payload
  2. A local diagnostic reports whether config loaded, provider resolved, library file present, library loaded, and listener bound, using only stage booleans, provider alias, ports, uptime, and error class
  3. Existing v0.2.0 clients continue to work unmodified through a protocol version indicator, and methods restricted for safety remain present and return a documented error with a migration note
  4. The Windows release ZIP ships with a SHA-256 checksum file, and the workflow verifies extracted contents plus `Machine=0x014C` for both the executable and the GM3000 DLL
  5. The threat model and Chinese/English documentation describe the trust boundary, session/authorization model, limits, restricted methods, and the post-refactor module layout
**Plans**: TBD

Plans:
- [ ] 06-01: TBD
- [ ] 06-02: TBD
- [ ] 06-03: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → 4 → 5 → 6

Phase 5 is file-disjoint from Phases 2-4 and may be executed in parallel if scheduling allows.

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Structural Foundation and Test Seam | 4/4 | Complete    | 2026-09-11 |
| 2. Session Authorization and Resource Ownership | 3/4 | In Progress|  |
| 3. Transport Hardening and Concurrency | 0/2 | Not started | - |
| 4. Format/Lint Normalization and CI Gate | 0/2 | Not started | - |
| 5. Windows Service Reliability | 0/2 | Not started | - |
| 6. Observability, Diagnostics, and Release Verification | 0/3 | Not started | - |

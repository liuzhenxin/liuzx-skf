# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

SKF Service 是一个 Rust 服务，通过 WebSocket 和 HTTP 提供统一 API，用于访问多厂商 USB Key 硬件安全模块（HSM）。服务在运行时动态加载厂商专属 SKF 共享库，并通过 WebSocket 以 JSON-RPC 2.0 协议对外暴露。

- WebSocket 服务：`ws://127.0.0.1:9001`
- HTTP API 演示：`http://0.0.0.0:8000`

## 构建与运行

```bash
cargo build              # debug 构建
cargo build --release    # release 构建
cargo run                # 使用默认配置运行
RUST_LOG=debug cargo run # 开启 debug 日志运行
```

## 测试

集成测试为 Node.js 脚本，通过 WebSocket 调用服务。执行测试前服务必须已启动。

```bash
# 完整测试流程（编译 → 部署 → 重启服务 → 测试 → 清理）
./tests/run_tests.sh

# 跳过服务重启（服务已在运行时使用）
SKF_NO_RESTART=1 ./tests/run_tests.sh

# 自定义 PIN（默认：12345678）
SKF_PIN=87654321 ./tests/run_tests.sh

# 自定义 WebSocket 地址（默认：ws://127.0.0.1:9001）
SKF_WS_URL=ws://192.168.1.100:9001 ./tests/run_tests.sh

# USB 插拔事件测试（持续运行，Ctrl+C 退出）
node tests/test_usb_event.js
```

## 代码质量

```bash
cargo fmt       # 格式化代码
cargo clippy    # 代码检查
```

## 交叉编译（Windows 目标）

`.cargo/config.toml` 配置了使用 MinGW 交叉编译到 Windows 的链接器：
- `x86_64-pc-windows-gnu` → `x86_64-w64-mingw32-gcc`
- `i686-pc-windows-gnu` → `i686-w64-mingw32-gcc`

## 架构

### 源码结构

```
src/
├── main.rs          # WebSocket 服务器、JSON-RPC 分发、全部业务逻辑（约 2000 行）
└── skf/
    ├── mod.rs       # 模块重导出
    ├── api.rs       # 动态加载 SKF 库函数的 FFI 封装
    └── types.rs     # C 兼容类型（ULONG、BYTE、HANDLE、ECCPUBLICKEYBLOB 等）
```

### 运行时流程

1. `main.rs` 将 `config/skf.yaml` 加载为 `SkfConfig`
2. 每个 WebSocket 连接创建一个 `SkfContext`（持有已加载的 `SkfApi` 实例和 PIN 缓存）
3. 收到的 JSON-RPC 2.0 消息分发到对应处理函数
4. 处理函数调用 `SkfApi` 方法，进而调用动态加载的厂商 `.dll`/`.so`/`.dylib` 中的 FFI 函数

### 动态库加载

`src/skf/api.rs` 封装了 `libloading::Library`。每个 `SkfApi` 实例持有 `Arc<Library>`，按需解析函数指针。若符号不存在，返回 `SAR_COULDNOTGETFUNCADDR` 而非 panic。

### 厂商配置（`config/skf.yaml`）

```yaml
default: "GM3000"          # 默认提供商
vendor:
  "055c:e618": "GM3000"    # VID:PID → 提供商别名
GM3000:
  windows: "%ProgramFiles(X86)%\\GM3000\\mtoken_gm3000.dll"
  linux:   "native/GM3000/linux/libgm3000.1.0.so"
  macos:   "native/GM3000/macos/x86_64/libgm3000.1.0.dylib"
```

库路径支持 `%VAR%` 环境变量展开。原生库存放于 `native/<厂商名>/<os>/`。

### 关键实现细节

- **DER 编码**（main.rs ~218–382）：手写 X.509 结构的 DER 编码，包括 SubjectDN、SM2 和 RSA 的 SPKI，未使用外部 ASN.1 库。
- **双证书模式**（main.rs ~880–1054）：生成 SM2 密钥对，用随机会话密钥 SM4-ECB 加密私钥，再用签名公钥 SM2 加密会话密钥，最终产生 `ENVELOPEDKEYBLOB`。
- **CSR/证书签发**：通过 shell 调用系统 `openssl` 命令完成 PKCS#10 和 Mock CA 操作。
- **PIN 缓存**：PIN 按设备+应用+容器为键缓存在 `SkfContext` 内存中，服务重启后失效。
- **双语错误消息**：`SetLanguage("CN"|"EN")` 切换当前会话的错误消息语言。

### 支持的算法

| 算法 | 用途 |
|------|------|
| SM2 | 密钥生成、签名、加密（基于 ECC 的国密标准） |
| SM3 | 哈希 |
| SM4 | 对称加密（ECB/CBC） |
| RSA | 密钥生成、签名 |
| SHA1/SHA256 | 哈希 |

### JSON-RPC 2.0 协议

所有参数为位置数组。示例：

```json
{ "jsonrpc": "2.0", "method": "EnumDevice", "params": ["GM3000"], "id": 1 }
```

主要方法：`EnumProvider`、`EnumDevice`、`ConnectDev`、`DisConnectDev`、`EnumApplication`、`EnumContainer`、`CheckPIN`、`FindCertificates`、`SignData`、`Digest`、`CreatePKCS10`、`IssueCertificate`、`ImportCertificate`、`ImportKeyPair`、`GenerateRandom`、`WaitForDevEvent`、`DeleteContainer`、`SetLanguage`。

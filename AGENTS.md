# AGENTS.md - 项目上下文文档

## 项目概述

SKF Service 是一个基于 Rust 开发的统一 SKF 接口服务，通过 WebSocket 和 HTTP 对外提供统一的调用接口，支持多厂商 USB Key 硬件安全模块（HSM）的动态切换。

### 核心功能
- 统一的 WebSocket/HTTP API，通过 JSON-RPC 2.0 协议访问 USB Key
- 多厂商支持：根据配置或运行时动态加载不同的 SKF 库（如 GM3000、3000GM）
- 跨平台支持：Windows、Linux、macOS（取决于厂商驱动）
- 国密支持：SM2 签名/加密、SM3 哈希、SM4 对称加密
- 证书管理：创建 PKCS#10 证书签名请求、导入证书、导入密钥对
- 设备事件监听：支持 USB Key 插拔事件的实时监听

### 技术栈
- **后端**：Rust 2021 Edition
  - 异步运行时：tokio
  - WebSocket：tokio-tungstenite
  - Web 框架：warp
  - 动态库加载：libloading
  - 序列化：serde (JSON/YAML)
  - 国密算法：smcrypto
  - 证书解析：x509-parser
- **前端/测试**：Node.js
  - WebSocket 客户端：ws
- **配置**：YAML (serde_yaml)

### 项目架构
```
liuzx-skf/
├── src/
│   ├── main.rs          # 主程序入口，WebSocket 服务器
│   └── skf/
│       ├── mod.rs       # 模块声明
│       ├── api.rs       # SKF API 接口定义
│       └── types.rs     # 类型定义（CHAR, ULONG, BYTE, DEVHANDLE 等）
├── config/
│   └── skf.yaml         # 服务配置文件
├── api/                 # HTTP API Demo 文件
├── tests/               # 集成测试脚本
├── native/              # 厂商动态库
└── docs/                # 文档
```

## 配置说明

服务通过 `config/skf.yaml` 进行配置，主要包含以下部分：

### 配置结构
```yaml
default: "GM3000"                    # 默认提供商别名

vendor:                              # 映射 USB 设备 VID:PID 到提供商
  "055c:e618": "GM3000"
  "096e:0309": "3000GM"

GM3000:                              # 提供商配置
  windows: "%ProgramFiles(X86)%\\GM3000\\mtoken_gm3000.dll"
  linux:  "native/GM3000/linux/libgm3000.1.0.so"
  macos: "native/GM3000/macos/x86_64/libgm3000.1.0.dylib"

3000GM:
  windows: "%System32%\\ShuttleCsp11_3000GM.dll"
  linux: "native/3000GM/linux/libes_3000gm.so.1.0.0"
  macos: "native/3000GM/macos/x86_64/libes_3000gm.dylib"
```

### 环境变量支持
配置文件中的库路径支持环境变量，使用 `%VAR%` 格式，如 `%ProgramFiles(X86)%`。

## 构建和运行

### 前置要求
- Rust 工具链（1.70+）
- Node.js（用于测试）
- USB Key 设备及相应驱动程序

### 构建项目
```bash
cargo build          # Debug 构建
cargo build --release  # Release 构建
```

### 运行服务
```bash
cargo run
```

服务启动后将监听：
- WebSocket：`ws://127.0.0.1:9001`
- HTTP API Demo：`http://0.0.0.0:8000`

### 运行测试
```bash
# 一键测试（自动编译 → 部署 → 重启服务 → 运行测试 → 清理）
./tests/run_tests.sh

# 跳过服务重启（适用于服务已在运行的情况）
SKF_NO_RESTART=1 ./tests/run_tests.sh

# 自定义 PIN 码（默认: 12345678）
SKF_PIN=87654321 ./tests/run_tests.sh

# 自定义 WebSocket 地址（默认: ws://127.0.0.1:9001）
SKF_WS_URL=ws://192.168.1.100:9001 ./tests/run_tests.sh

# USB Key 插拔事件监听（独立运行，Ctrl+C 退出）
node tests/test_usb_event.js
```

## API 文档

### WebSocket API 端点
- URL：`ws://127.0.0.1:9001`
- 协议：JSON-RPC 2.0
- 请求格式：所有参数按数组形式传递

### 主要 API 方法

#### 设备管理
- **`SetLanguage`**: 设置错误消息语言
  - 参数：`["CN"]` 或 `["EN"]`
- **`EnumProvider`**: 列出支持的提供商或查询特定设备的提供商
  - 参数：`[]` (列出所有) 或 `["VID:PID"]` (查询特定)
- **`EnumDevice`**: 枚举特定提供商的设备
  - 参数：`["ProviderAlias"]`
- **`WaitForDevEvent`**: 阻塞并等待设备插拔事件
  - 参数：`["ProviderAlias"]`
  - 响应：`{ deviceName: string, event: number }` (1=插入, 2=拔出)
- **`ConnectDev`**: 连接到特定设备
  - 参数：`["DeviceName"]`
  - 响应：设备句柄（字符串格式的整数）
- **`DisConnectDev`**: 断开设备连接
  - 参数：`[DeviceHandle]`

#### 应用和容器管理
- **`EnumApplication`**: 枚举特定设备上的应用
  - 参数：`["ProviderAlias", "DeviceName"]`
- **`EnumContainer`**: 枚举应用中的容器
  - 参数：`["ProviderAlias", "DeviceName", "AppName"]`
- **`DeleteContainer`**: 删除指定容器
  - 参数：`["ProviderAlias", "DeviceName", "AppName", "ContainerName", "PIN"]`

#### 证书和密钥管理
- **`FindCertificates`**: 搜索所有连接的设备和容器中的证书
  - 参数：`["Filter"]` (可选: `"Sign"`, `"Enc"`, 或留空表示两者)
  - 响应：证书对象数组 (`{key, value, type, cert}`)
- **`SignData`**: 使用指定的证书签名数据
  - 参数：`["CertKeyPath", "DataBase64", "PIN"]`
- **`Digest`**: 使用设备计算哈希摘要
  - 参数：`["ProviderAlias", "DeviceName", "DataBase64", "Alg"]` (Alg: "SM3", "SHA1", "SHA256")
- **`CreatePKCS10`**: 创建 PKCS#10 证书签名请求 (CSR)
  - 参数：`["ProviderAlias", "DeviceName", "AppName", "PIN", "SubjectDN", "KeyType", KeyLength]`
- **`IssueCertificate`**: 根据 CSR 颁发证书（仅用于测试/Mock）
  - 参数：`["CSR", DoubleBool]`
- **`ImportCertificate`**: 向容器中导入证书
  - 参数：`["ProviderAlias", "DeviceName", "AppName", "ContainerName", "PIN", IsSignCertBool, "CertData"]`
- **`ImportKeyPair`**: 向容器中导入加密密钥对
  - 参数：`["ProviderAlias", "DeviceName", "AppName", "ContainerName", "PIN", "Alg", "EncKeyPairBase64", "WrapKeyBase64"]`
- **`GenerateRandom`**: 使用设备生成随机数
  - 参数：`[DeviceHandle, Length]`

### API 使用示例

```json
{
  "jsonrpc": "2.0",
  "method": "EnumDevice",
  "params": ["GM3000"],
  "id": 1
}
```

## 开发规范

### 代码风格
- 遵循 Rust 官方代码风格指南（rustfmt）
- 使用 `cargo fmt` 格式化代码
- 使用 `cargo clippy` 进行代码检查

### 测试实践
- 集成测试使用 Node.js 编写，通过 WebSocket 调用服务
- 测试覆盖单证书和双证书的完整安装流程
- USB Key 插拔事件测试独立运行
- 测试脚本支持环境变量配置（PIN、WebSocket URL 等）

### 错误处理
- 使用 `anyhow` crate 进行错误处理
- 支持 中英文错误消息切换
- 所有 API 调用返回 JSON-RPC 格式的响应，包含错误码和消息

### 日志
- 使用 `env_logger` 和 `log` crate
- 通过环境变量 `RUST_LOG` 控制日志级别
- 示例：`RUST_LOG=debug cargo run`

### 安全注意事项
- 动态库加载使用 `unsafe` 块，需要确保库来源可信
- PIN 码在内存中缓存，服务重启后需重新验证
- 设备句柄是字符串形式的指针，需要谨慎处理

## 常见问题

### Q: 如何添加新的 USB Key 厂商支持？
A: 在 `config/skf.yaml` 中添加新的厂商配置，包括 VID:PID 映射和各平台的库路径。

### Q: 服务无法加载动态库怎么办？
A: 检查配置文件中的库路径是否正确，确保环境变量已正确设置，动态库文件存在且可访问。

### Q: 如何调试 API 调用？
A: 设置 `RUST_LOG=debug` 环境变量运行服务，查看详细的调试日志。

### Q: 支持哪些加密算法？
A: 目前支持国密算法（SM2、SM3、SM4）和 RSA 算法。

## 相关文档
- 中文文档：`README_CN.md`
- 英文文档：`README_EN.md`
- FAQ：`docs/faq.md`

## 维护者
项目使用 Rust 和 Node.js 混合开发，主要维护者需要熟悉：
- Rust 异步编程
- WebSocket 协议
- 国密算法
- USB Key 设备管理
- PKI 证书管理
## 重构后的模块布局与本地诊断

- `protocol/`、`domain/`、`session/`、`provider/`、`service_state/`、`logging/`、
  `diagnostic/`、`server/`、`config/`、`crypto/`、`skf/`（权威列表见 `src/lib.rs`）。
- 本地诊断：`skf-service.exe diagnose [--config <path>] [--json]`，只输出阶段布尔、
  provider 别名、端口、uptime 与错误类别，不泄露库路径/配置值/凭据。
- 日志：结构化 JSON 行写入 `<install>/logs/skf-service.log`，10 MiB 轮转保留 7 个；
  `tests/log_redaction.rs` 扫描源码阻止敏感值进入日志宏。
- 门禁与限制：`docs/CI.md`、`THREAT-MODEL.md`、`docs/SESSION-AND-LIMITS.md`。

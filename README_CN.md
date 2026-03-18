# SKF Service (Rust)

基于 Rust 的 SKF 接口服务，通过 WebSocket / HTTP 对外提供统一调用接口，支持多厂商 SKF 动态切换（如 GM3000、3000GM）。

## 特性

- **统一 WebSocket / HTTP API**：通过简单的 JSON-RPC 接口访问硬件安全模块 (HSM) / USB Key。
- **多厂商支持**：根据配置或运行时选择动态加载不同的 SKF 库。
- **跨平台**：支持 Windows、Linux 和 macOS（取决于驱动）。

## 配置 (`config/skf.yaml`)

服务通过 `config/skf.yaml` 进行配置。

- **default**：未指定时使用的默认提供商别名。
- **vendor**：映射 USB 设备 VendorID:ProductID (VID:PID) 到提供商别名。
- **`<ProviderAlias>`**：定义特定提供商在不同操作系统上的库路径。

示例：

```yaml
default: "GM3000"

vendor:
  "055c:e618": "GM3000"

GM3000:
  windows: "%ProgramFiles(X86)%\\GM3000\\mtoken_gm3000.dll"
  linux:  "native/GM3000/linux/libgm3000.1.0.so"
  macos: "native/GM3000/macos/x86_64/libgm3000.1.0.dylib"
```

## 运行

```bash
cargo run
```

服务将监听 `ws://127.0.0.1:9001` (WebSocket)，并在 `http://0.0.0.0:8000` 提供 HTTP API Demo。

## WebSocket API

服务使用 JSON-RPC 2.0 风格的消息。所有参数均按数组形式传递。

### 端点

`ws://127.0.0.1:9001`

### 方法

- **`SetLanguage`**: 设置错误消息语言。
  - 参数: `["CN"]` 或 `["EN"]`
- **`EnumProvider`**: 列出支持的提供商或查询特定设备的提供商。
  - 参数: `[]` (列出所有) 或 `["VID:PID"]` (查询特定)
- **`EnumDevice`**: 列出特定提供商的设备。
  - 参数: `["ProviderAlias"]`
- **`WaitForDevEvent`**: 阻塞并等待设备插拔事件。
  - 参数: `["ProviderAlias"]`
  - 响应: `{ deviceName: string, event: number }` (1=插入, 2=拔出)
- **`EnumApplication`**: 枚举特定设备上的应用。
  - 参数: `["ProviderAlias", "DeviceName"]`
- **`EnumContainer`**: 枚举应用中的容器。
  - 参数: `["ProviderAlias", "DeviceName", "AppName"]`
- **`DeleteContainer`**: 删除指定容器。
  - 参数: `["ProviderAlias", "DeviceName", "AppName", "ContainerName", "PIN"]`
- **`ConnectDev`**: 连接到特定设备。
  - 参数: `["DeviceName"]`
  - 响应: 设备句柄（字符串格式的整数）
- **`DisConnectDev`**: 断开设备连接。
  - 参数: `[DeviceHandle]`
- **`FindCertificates`**: 搜索所有连接的设备和容器中的证书。
  - 参数: `["Filter"]` (可选: `"Sign"`, `"Enc"`, 或留空表示两者)
  - 响应: 证书对象数组 (`{key, value, type, cert}`)
- **`SignData`**: 使用指定的证书签名数据。
  - 参数: `["CertKeyPath", "DataBase64", "PIN"]`
- **`Digest`**: 使用设备计算哈希摘要。
  - 参数: `["ProviderAlias", "DeviceName", "DataBase64", "Alg"]` (Alg: "SM3", "SHA1", "SHA256")
- **`CreatePKCS10`**: 创建 PKCS#10 证书签名请求 (CSR)。
  - 参数: `["ProviderAlias", "DeviceName", "AppName", "PIN", "SubjectDN", "KeyType", KeyLength]`
- **`IssueCertificate`**: 根据 CSR 颁发证书（仅用于测试/Mock）。
  - 参数: `["CSR", DoubleBool]`
- **`ImportCertificate`**: 向容器中导入证书。
  - 参数: `["ProviderAlias", "DeviceName", "AppName", "ContainerName", "PIN", IsSignCertBool, "CertData"]`
- **`ImportKeyPair`**: 向容器中导入加密密钥对。
  - 参数: `["ProviderAlias", "DeviceName", "AppName", "ContainerName", "PIN", "Alg", "EncKeyPairBase64", "WrapKeyBase64"]`
- **`GenerateRandom`**: 使用设备生成随机数。
  - 参数: `[DeviceHandle, Length]`

## 使用示例

连接到 WebSocket 并发送:

```json
{
  "jsonrpc": "2.0",
  "method": "EnumDevice",
  "params": ["GM3000"],
  "id": 1
}
```

## 测试

项目包含集成测试脚本，用于验证单证书和双证书的完整安装流程。

### 前提条件

- 已插入 USB Key 设备
- Node.js 环境可用
- `ws` 模块（脚本会自动安装）

### 运行测试

```bash
# 一键测试（自动编译 → 部署 → 重启服务 → 运行测试 → 清理）
./tests/run_tests.sh
```

### 可选参数

```bash
# 跳过服务重启（适用于服务已在运行的情况）
SKF_NO_RESTART=1 ./tests/run_tests.sh

# 自定义 PIN 码（默认: 12345678）
SKF_PIN=87654321 ./tests/run_tests.sh

# 自定义 WebSocket 地址（默认: ws://127.0.0.1:9001）
SKF_WS_URL=ws://192.168.1.100:9001 ./tests/run_tests.sh
```

### 测试内容

| 测试项 | 说明 |
|--------|------|
| 枚举设备提供者 | 验证 `EnumProvider` 返回可用提供商 |
| 枚举/连接设备 | 验证设备发现与连接 |
| 单证书安装 | `CreatePKCS10` → `IssueCertificate` → `ImportCertificate` |
| 双证书安装 | `CreatePKCS10` → `IssueCertificate(double)` → `ImportKeyPair` → `ImportCertificate(签名)` → `ImportCertificate(加密)` |

### USB Key 插拔事件监听

独立的事件监听脚本，实时打印设备插入/拔出事件，按 `Ctrl+C` 退出：

```bash
node tests/test_usb_event.js
```

### 测试文件说明

| 文件 | 说明 |
| ---- | ---- |
| `tests/run_tests.sh` | 一键运行所有自动化测试 |
| `tests/test_cert_install.js` | 基础证书安装测试（单/双证书） |
| `tests/test_skf_api.js` | 基于 `SKFClient` 的全量 API 测试 |
| `tests/test_usb_event.js` | USB Key 插拔事件监听器 |
| `tests/verify_csr_subject.js` | 验证 CSR 主题信息的脚本 |

## 联系方式

如果您在使用过程中遇到任何问题，欢迎通过微信扫码联系：

![WeChat QR Code](docs/wechat_qr.png)

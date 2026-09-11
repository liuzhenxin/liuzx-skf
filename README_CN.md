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
  windows: "native\\GM3000\\windows\\mtoken_gm3000.dll"
  linux:  "native/GM3000/linux/libgm3000.1.0.so"
  macos: "native/GM3000/macos/x86_64/libgm3000.1.0.dylib"
```

## 运行

```bash
cargo run
```

服务将监听 `ws://127.0.0.1:9001` (WebSocket)，并在 `http://0.0.0.0:8000` 提供 HTTP API Demo。

## Windows x64 独立运行包（GM3000）

### 架构要求

仓库中的 `native/GM3000/windows/mtoken_gm3000.dll` 已确认为
**PE32/i386（32 位）**。Windows x64 可以通过 WoW64 运行 32 位程序，但一个
64 位进程不能加载该 DLL。因此部署架构必须保持如下组合：

| 组件 | 架构 |
| --- | --- |
| Windows 操作系统 | x86_64 |
| `skf-service.exe` | x86 / i686 |
| `mtoken_gm3000.dll` | x86 / i386 |

如果 `skf-service.exe` 的 PE Machine 是 `0x8664`，调用 `EnumDevice` 时会出现
`LoadLibraryExW failed`。正确的 exe 和 DLL 都应为 `0x014C`。

### 构建环境

请在 Windows x64 构建机安装：

- [Rust](https://rustup.rs/)；
- Visual Studio Build Tools；
- “使用 C++ 的桌面开发”；
- MSVC v143 x86/x64 构建工具与 Windows SDK。

目标计算机运行时不需要 Rust、Cargo 或 Node.js，但必须安装 GM3000 USB Key
硬件驱动；随包的用户态 SKF DLL 不能替代 USB/HID 内核驱动。

### 一键构建与打包

在工程根目录的 PowerShell 中执行：

```powershell
cd D:\liuzx-skf
powershell -ExecutionPolicy Bypass -File .\packaging\windows\build.ps1
```

脚本默认执行：

```powershell
rustup target add i686-pc-windows-msvc
cargo build --release --target i686-pc-windows-msvc
```

并使用静态 MSVC CRT，随后校验 exe 和 DLL 均为 PE Machine `0x014C`。输出：

```text
dist\skf-service-windows-x64-gm3000-x86\
dist\skf-service-windows-x64-gm3000-x86.zip
```

完整打包脚本和运行包说明见
[`packaging/windows/README-Windows-x86_64.md`](packaging/windows/README-Windows-x86_64.md)。

### 运行包结构

```text
skf-service-windows-x64-gm3000-x86\
├── skf-service.exe
├── run.bat
├── install.bat
├── install.ps1
├── uninstall.bat
├── uninstall.ps1
├── config\skf.yaml
├── native\GM3000\windows\mtoken_gm3000.dll
└── api\
```

运行包中的配置必须从随包路径加载 DLL：

```yaml
GM3000:
  windows: "native\\GM3000\\windows\\mtoken_gm3000.dll"
```

### 便携运行与服务安装

解压后可直接双击 `run.bat` 前台运行。要安装为开机自启服务，右键
`install.bat` 并选择“以管理员身份运行”。安装脚本会复制到：

```text
C:\Program Files (x86)\LiuZX\SKF Service
```

注册并立即启动 `LiuZXSKFService`，同时配置崩溃自动重启。常用命令：

```powershell
sc.exe query LiuZXSKFService
Restart-Service LiuZXSKFService
Test-NetConnection 127.0.0.1 -Port 9001
```

服务日志：

```text
C:\Program Files (x86)\LiuZX\SKF Service\skf-service.log
```

卸载时右键管理员运行 `uninstall.bat`。

### 检查 PE 架构

普通 PowerShell 不一定包含 `dumpbin`，可直接读取 PE Machine：

```powershell
$files = @(
  '.\dist\skf-service-windows-x64-gm3000-x86\skf-service.exe',
  '.\dist\skf-service-windows-x64-gm3000-x86\native\GM3000\windows\mtoken_gm3000.dll'
)
foreach ($path in $files) {
    $bytes = [System.IO.File]::ReadAllBytes((Resolve-Path $path))
    $peOffset = [BitConverter]::ToInt32($bytes, 0x3C)
    $machine = [BitConverter]::ToUInt16($bytes, ($peOffset + 4))
    "{0}: Machine=0x{1:X4}" -f $path, $machine
}
```

两个文件都应输出 `Machine=0x014C`。

### `LoadLibraryExW failed` 排查

1. 检查 exe 和 DLL 是否都为 `0x014C`；
2. 检查 `config/skf.yaml` 是否仍指向旧的
   `C:\Program Files (x86)\GM3000\mtoken_gm3000.dll`；
3. 检查随包 DLL 是否存在：

```powershell
Test-Path '.\native\GM3000\windows\mtoken_gm3000.dll'
```

4. 修改配置或替换 DLL 后重启服务，清除已缓存的动态库状态。

如果客户端运行在宿主机而服务运行在虚拟机，`127.0.0.1` 指向宿主机而不是
虚拟机。此时需要设置 `SKF_WS_ADDR=0.0.0.0:9001`、开放防火墙端口，并使用
`ws://<虚拟机IP>:9001` 连接。

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
- **`CheckPIN`**: 验证用户 PIN 码并缓存（后续操作无需重复验证）。
  - 参数: `["CertKeyPath", "PIN"]`
- **`CreateContainer`**: 创建新容器。
  - 参数: `["ProviderAlias", "DeviceName", "AppName", "ContainerName"]`
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
  - 参数: `["ProviderAlias", "DeviceName", "AppName", "ContainerName", "Alg", "EncKeyPairBase64", "WrapKeyBase64", "SM4Mode?"]`
  - `Alg`: "SM2"/"ECC" 或 "RSA"
  - `SM4Mode`: RSA 密钥导入时的 SM4 加密模式 ("ECB" 或 "CBC"，默认 ECB)
- **`EncryptData`**: 使用 SM4-CBC 算法加密数据。
  - 参数: `["CertKeyPath", "DataBase64", "IVBase64", "PaddingType", "SymKeyBase64?"]`
  - `PaddingType`: 1=PKCS#7 填充 (默认), 0=无填充
  - `SymKeyBase64`: 可选的外部对称密钥 (16 字节)，不提供则使用容器内的密钥
- **`DecryptData`**: 使用 SM4-CBC 算法解密数据。
  - 参数: `["CertKeyPath", "EncryptedDataBase64", "IVBase64", "PaddingType", "SymKeyBase64?"]`
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

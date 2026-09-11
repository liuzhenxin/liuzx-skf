# LiuZX SKF Service — Windows x64 独立运行包

本运行包面向 **Windows x86_64 操作系统**。由于随包的 GM3000
`mtoken_gm3000.dll` 是 PE32/i386，`skf-service.exe` 也必须构建为 32 位；
它会通过 Windows WoW64 正常运行和注册为 Windows 服务。

## 运行时架构

| 组件 | 架构 |
| --- | --- |
| Windows | x86_64 |
| skf-service.exe | x86 / i686 |
| mtoken_gm3000.dll | x86 / i386 |

64 位进程不能加载该 DLL。不要把服务程序替换为 x86_64 版本。

## 前置条件

- Windows 10/11 x64 或 Windows Server x64。
- 已安装 GM3000 USB Key 厂商驱动。运行包包含用户态 SKF DLL，但不能替代
  USB/HID 内核驱动。
- 目标计算机运行程序时不需要安装 Rust、Cargo 或 Node.js。

## 便携运行（不安装）

解压后双击：

```text
run.bat
```

服务端点：

- WebSocket：`ws://127.0.0.1:9001`
- HTTP Demo：`http://127.0.0.1:8000/skf_api.html`

关闭控制台或按 `Ctrl+C` 可停止程序。

## 安装为开机自启服务

右键 `install.bat`，选择“以管理员身份运行”。安装程序会：

1. 复制文件到 `C:\Program Files (x86)\LiuZX\SKF Service`；
2. 注册服务 `LiuZXSKFService`；
3. 设置启动类型为 `Automatic`；
4. 立即启动服务；
5. 配置服务崩溃后自动重启。

检查状态：

```bat
sc query LiuZXSKFService
"C:\Program Files (x86)\LiuZX\SKF Service\skf-service.exe" status
```

日志：

```text
C:\Program Files (x86)\LiuZX\SKF Service\skf-service.log
```

卸载时右键 `uninstall.bat`，选择“以管理员身份运行”。

## 构建运行包

请在 Windows x64 构建机上安装 Rust 和 Visual Studio Build Tools，然后执行：

```powershell
cd packaging\windows
powershell -ExecutionPolicy Bypass -File .\build.ps1
```

构建脚本使用：

```text
i686-pc-windows-msvc + 静态 MSVC CRT
```

输出：

```text
dist\skf-service-windows-x64-gm3000-x86\
dist\skf-service-windows-x64-gm3000-x86.zip
```

## 运行包结构

```text
skf-service-windows-x64-gm3000-x86\
├── skf-service.exe
├── run.bat
├── install.bat
├── install.ps1
├── uninstall.bat
├── uninstall.ps1
├── config\
│   └── skf.yaml
├── native\
│   └── GM3000\windows\mtoken_gm3000.dll
└── api\
    ├── skf_api.html
    └── skf_api.js
```

## 构建结果检查

出现下面两行表示 Rust 编译和运行包组装成功；C ABI 命名产生的 warning 不影响
运行：

```text
Finished `release` profile [optimized]
Build complete
```

当前脚本的输出目录必须是 `skf-service-windows-x64-gm3000-x86`。如果仍显示
`skf-service-windows-x86_64`，说明 Windows 构建机使用的是旧版
`packaging/windows/build.ps1`，应先同步整个 `packaging/windows/` 目录。

普通 PowerShell 可使用以下代码检查 PE Machine，不要求安装 `dumpbin`：

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

两个文件都必须为 `0x014C`。`skf-service.exe=0x8664`、DLL=`0x014C` 是明确的
位数不匹配，必须从
`target\i686-pc-windows-msvc\release\skf-service.exe` 重新复制程序。

## `LoadLibraryExW failed` 排查

| 现象 | 原因 | 处理 |
| --- | --- | --- |
| exe=`0x8664`，DLL=`0x014C` | 64 位进程加载 32 位 DLL | 使用 `i686-pc-windows-msvc` 重建 |
| 日志加载 `C:\Program Files (x86)\GM3000\...` | 使用了旧配置 | 改为随包相对路径并重启服务 |
| `Test-Path` 返回 `False` | 运行包缺少 DLL | 重新执行完整打包脚本 |
| exe、DLL 均为 `0x014C` 仍失败 | 驱动未安装或 DLL 初始化失败 | 安装 GM3000 驱动并查看 `skf-service.log` 中的 Windows 错误码 |

正确配置：

```yaml
GM3000:
  windows: "native\\GM3000\\windows\\mtoken_gm3000.dll"
```

检查文件：

```powershell
Test-Path '.\native\GM3000\windows\mtoken_gm3000.dll'
```

修改配置、替换 DLL 或替换 exe 后必须重启进程/服务。

## 注意

`IssueCertificate` 是测试/Mock 接口，当前会调用外部 `openssl.exe`。若目标计算机
没有安装 OpenSSL，该接口不可用；设备枚举、CSR、PIN、签名、证书导入等 SKF
核心接口不依赖 OpenSSL。

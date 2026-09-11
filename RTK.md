# RTK.md - liuzx-skf

## 1. 项目用途

`liuzx-skf` 是 Rust 实现的统一 SKF 接口服务，通过 WebSocket 和 HTTP Demo 对外提供 JSON-RPC 2.0 能力，动态加载多厂商 USB Key/SKF 动态库，支持 SM2、SM3、SM4、证书管理、CSR 创建、证书导入和设备事件监听。

## 2. 目录结构说明

```text
liuzx-skf/
├── src/main.rs       # 服务入口
├── src/win_service.rs # Windows SCM 服务注册、启停和自启动
├── src/skf/          # SKF API、类型定义和调用封装
├── config/skf.yaml   # 厂商和动态库路径配置
├── packaging/windows/ # Windows 构建、便携运行和服务安装脚本
├── api/              # HTTP API Demo
├── tests/            # Node.js 集成测试脚本
├── native/           # 厂商动态库放置目录
└── docs/             # 说明文档
```

## 3. 如何安装依赖

```bash
cargo build
```

Node.js 仅用于测试脚本。USB Key 集成测试需要目标厂商驱动和动态库可用。

仓库中的 GM3000 `mtoken_gm3000.dll` 为 PE32/i386。即使目标操作系统是
Windows x64，服务程序也必须构建为 i686，保证 exe 和 DLL 均为 PE Machine
`0x014C`：

```powershell
rustup target add i686-pc-windows-msvc
cargo build --release --target i686-pc-windows-msvc
```

推荐直接运行可复现的打包脚本（使用静态 MSVC CRT，并自动校验 PE 架构）：

```powershell
powershell -ExecutionPolicy Bypass -File .\packaging\windows\build.ps1
```

产物为 `dist\skf-service-windows-x64-gm3000-x86.zip`。目标机不需要安装
Rust 或 Node.js，但仍需安装 GM3000 硬件驱动。

## 4. 如何运行项目

```bash
cargo run
```

默认监听：

```text
WebSocket: ws://127.0.0.1:9001
HTTP Demo: http://0.0.0.0:8000
```

可用 `RUST_LOG=debug cargo run` 查看调试日志。

Windows 运行包同时支持：

- `run.bat`：不安装，前台便携运行；
- `install.bat`：管理员运行，安装 `LiuZXSKFService`、立即启动并设置开机自启；
- `uninstall.bat`：停止并删除服务。

Windows 服务默认安装到 `C:\Program Files (x86)\LiuZX\SKF Service`，日志为
`skf-service.log`。详细说明见
`packaging/windows/README-Windows-x86_64.md`。

## 5. 如何测试

```bash
cargo test
cargo build
./tests/run_tests.sh
```

集成测试常用环境变量：

```bash
SKF_NO_RESTART=1 ./tests/run_tests.sh
SKF_PIN=87654321 ./tests/run_tests.sh
SKF_WS_URL=ws://127.0.0.1:9001 ./tests/run_tests.sh
```

PIN 属敏感信息，不要写入提交或日志。

## 6. 主要模块说明

- `main.rs`: WebSocket/HTTP 服务启动、资源路径处理和请求分发。
- `win_service.rs`: Windows SCM 服务注册、状态管理、停止通知和日志重定向。
- `skf/api.rs`: SKF API 抽象和 JSON-RPC 方法映射。
- `skf/types.rs`: SKF C 接口类型。
- `config/skf.yaml`: 厂商别名、VID/PID 映射和各平台动态库路径。
- `tests`: USB Key 集成测试和事件监听测试。

## 7. 常见开发注意事项

- 动态库加载涉及 `unsafe`，新增调用前确认 C ABI、句柄生命周期和错误码映射。
- 厂商库路径应通过配置和环境变量表达，不写死到通用代码。
- 设备句柄、PIN、证书私钥和密钥材料不得输出到日志。
- 提交前运行 `cargo fmt`、`cargo clippy`、`cargo test`，硬件测试结果需注明设备和厂商。

# 🚀 liuzx-skf：基于 Rust 的高性能、跨平台统一 SKF 服务框架已开源！

还在为不同厂商 USB Key 的 C 接口调用、动态库加载以及跨平台兼容性而烦恼吗？

**liuzx-skf** 正是为了解决这些痛点而生！我们使用 Rust 重新定义了 SKF 服务的接入方式：

✅ **统一接口**：通过 WebSocket / HTTP (JSON-RPC 2.0) 接口访问硬件安全模块 (HSM) / USB Key，告别复杂的 C 语言头文件。
✅ **厂商适配**：支持 GM3000、3000GM 等多厂商驱动动态加载与实时切换，一套代码跑通所有硬件。
✅ **极致性能**：利用 Rust 异步运行时，支持设备插拔事件实时监听，响应极速且内存安全。
✅ **国密全能**：深度集成 SM2 签名/加密、SM3 哈希、SM4 对称加密及证书完整生命周期管理。
✅ **现代 UI**：内置颜值爆表的 Demo 页面，采用现代化 HSL 透明磨砂玻璃感设计，调试如丝般顺滑。

无论是电子签名、金融收单还是高安全等级的单点登录，liuzx-skf 都是您最佳的底层基座。

🔗 **项目地址**：[https://github.com/liuzhenxin/liuzx-skf](https://github.com/liuzhenxin/liuzx-skf)

欢迎大家 **Star** 关注，也欢迎提 Issue 或 PR，一起完善国密开源生态！

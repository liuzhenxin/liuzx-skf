/**
 * USB Key 插拔事件监听器
 *
 * 持续监听 USB Key 的插入和拔出事件，实时打印事件信息。
 * 按 Ctrl+C 退出。
 *
 * 用法:
 *   node tests/test_usb_event.js
 *
 * 环境变量:
 *   SKF_WS_URL  - WebSocket 地址 (默认: ws://127.0.0.1:9001)
 */

const WS = require('ws');
global.WebSocket = WS;

const SKFClient = require('../api/skf_api');

const WS_URL = process.env.SKF_WS_URL || 'ws://127.0.0.1:9001';

async function run() {
    const skf = new SKFClient(WS_URL);

    console.log('========================================');
    console.log('  USB Key 插拔事件监听器');
    console.log('========================================');
    console.log(`  服务地址: ${WS_URL}`);
    console.log('  按 Ctrl+C 退出');
    console.log('----------------------------------------\n');

    try {
        await skf.connect();
        console.log('✅ 已连接到 SKF Service\n');
    } catch (e) {
        console.error(`❌ 连接失败: ${e.message}`);
        process.exit(1);
    }

    // 获取 provider
    const providers = await skf.enumProvider();
    if (providers.length === 0) {
        console.error('❌ 未找到设备提供者');
        skf.disconnect();
        process.exit(1);
    }
    const provider = providers[0];
    console.log(`使用提供者: ${provider}`);

    // 先枚举当前设备
    const devices = await skf.enumDevice(provider);
    if (devices.length > 0) {
        console.log(`当前已插入设备: ${devices.join(', ')}`);
    } else {
        console.log('当前无设备插入');
    }

    console.log('\n🔄 开始监听事件...\n');

    let eventCount = 0;

    // 持续监听循环
    while (true) {
        try {
            const event = await skf.waitForDevEvent(provider);
            eventCount++;
            const now = new Date().toLocaleTimeString('zh-CN');
            const eventType = event.event === 1 ? '🔌 插入' : '🔌 拔出';

            console.log(`[${now}] #${eventCount} ${eventType} - 设备: ${event.deviceName}`);

            // 插入事件时自动枚举设备
            if (event.event === 1) {
                try {
                    const devs = await skf.enumDevice(provider);
                    console.log(`         当前设备列表: ${devs.join(', ') || '(空)'}`);
                } catch (e) {
                    // 枚举失败不影响继续监听
                }
            }
        } catch (e) {
            // 连接断开或取消
            if (e.message && e.message.includes('Not connected')) {
                console.log('\n连接已断开');
                break;
            }
            console.error(`\n监听出错: ${e.message || JSON.stringify(e)}`);
            // 短暂等待后重试
            await new Promise(r => setTimeout(r, 1000));
        }
    }
}

// Ctrl+C 优雅退出
process.on('SIGINT', () => {
    console.log('\n\n停止监听');
    process.exit(0);
});

run().catch(e => {
    console.error('异常:', e);
    process.exit(1);
});

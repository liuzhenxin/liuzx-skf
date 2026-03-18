#!/bin/bash
#
# SKF Service 集成测试脚本
# 用法: ./tests/run_tests.sh
#
# 环境变量:
#   SKF_PIN         - 设备 PIN 码 (默认: 12345678)
#   SKF_WS_URL      - WebSocket 地址 (默认: ws://127.0.0.1:9001)
#   SKF_NO_RESTART  - 设置为 1 时跳过服务重启 (默认: 自动重启)
#

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DIST_DIR="$PROJECT_DIR/dist/skf-service-macos-x86_64"
SERVICE_BIN="$DIST_DIR/skf-service"
LOG_FILE="$DIST_DIR/skf_service.log"

echo "========================================"
echo "  SKF Service 集成测试 / SKF Service Integration Tests"
echo "========================================"
echo ""

# Step 1: 编译项目
echo "[1/4] 编译项目... / Compiling project..."
cd "$PROJECT_DIR"
cargo build 2>&1 | grep -E "(Compiling|Finished|error)" || true
echo "  编译完成 / Compilation finished"

# Step 2: 复制二进制文件
echo "[2/4] 部署二进制... / Deploying binary..."
cp "$PROJECT_DIR/target/debug/skf-service" "$SERVICE_BIN"
echo "  已部署到 / Deployed to: $SERVICE_BIN"

# Step 3: 重启服务
if [ "${SKF_NO_RESTART}" != "1" ]; then
    echo "[3/4] 重启服务... / Restarting service..."
    killall skf-service 2>/dev/null || true
    sleep 1
    "$SERVICE_BIN" > "$LOG_FILE" 2>&1 &
    SERVICE_PID=$!
    echo "  服务已启动 / Service started (PID: $SERVICE_PID)"
    sleep 2

    # 等待端口就绪
    for i in $(seq 1 10); do
        if lsof -i :9001 -P | grep -q LISTEN; then
            echo "  端口 9001 就绪 / Port 9001 is ready"
            break
        fi
        sleep 1
    done
else
    echo "[3/4] 跳过服务重启 / Skipping service restart (SKF_NO_RESTART=1)"
fi

# Step 4: 安装依赖 & 运行测试
echo "[4/4] 运行测试... / Running tests..."
echo ""

# 确保 ws 模块可用
if [ ! -d "$PROJECT_DIR/node_modules/ws" ]; then
    npm install --prefix "$PROJECT_DIR" ws --silent 2>/dev/null
fi

echo "--- SKF API 全量测试 / Consolidated SKF API Tests ---"
NODE_PATH="$PROJECT_DIR/node_modules" node "$SCRIPT_DIR/test_all_apis.js"
TEST_EXIT=$?

echo ""

# 清理: 如果是我们启动的服务则停止
if [ "${SKF_NO_RESTART}" != "1" ] && [ -n "$SERVICE_PID" ]; then
    echo "停止服务 / Stopping service (PID: $SERVICE_PID)..."
    kill $SERVICE_PID 2>/dev/null || true
    wait $SERVICE_PID 2>/dev/null || true
fi

if [ $TEST_EXIT -eq 0 ]; then
    echo "✅ 所有测试通过 / All tests passed successfully"
else
    echo "❌ 存在失败的测试 / Tests failed"
    echo ""
    echo "服务日志 / Service Logs:"
    tail -20 "$LOG_FILE"
fi

exit $TEST_EXIT

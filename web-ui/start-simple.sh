#!/bin/bash

echo "🚀 启动 Rust Agent Web UI (简化版)"

# 检查是否已安装依赖
if [ ! -d "node_modules" ]; then
    echo "📦 安装依赖..."
    npm install
fi

echo "🔧 启动开发服务器..."
echo "📱 访问地址: http://localhost:3000"
echo "🔗 WebSocket 服务器: ws://localhost:9527"
echo ""
echo "💡 提示:"
echo "   1. 这是一个简化版演示界面"
echo "   2. 完整功能需要安装所有依赖"
echo "   3. 按 Ctrl+C 停止服务器"

npm run dev
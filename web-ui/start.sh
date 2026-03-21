#!/bin/bash

# Rust Agent Web UI 快速启动脚本

echo "🚀 启动 Rust Agent Web UI 开发环境"

# 检查 Node.js 是否安装
if ! command -v node &> /dev/null; then
    echo "❌ Node.js 未安装，请先安装 Node.js"
    exit 1
fi

# 检查 npm 是否安装
if ! command -v npm &> /dev/null; then
    echo "❌ npm 未安装，请先安装 npm"
    exit 1
fi

echo "📦 安装依赖..."
# 使用淘宝镜像加速
npm config set registry https://registry.npmmirror.com

# 安装依赖
npm install --verbose

if [ $? -eq 0 ]; then
    echo "✅ 依赖安装完成"
    
    echo "🔧 启动开发服务器..."
    echo "📱 访问地址: http://localhost:3000"
    echo "🔗 WebSocket 服务器: ws://localhost:9527"
    echo ""
    echo "💡 提示: 请确保 Rust Agent 服务器已启动:"
    echo "       ./target/release/agent --mode server --host 0.0.0.0 --port 9527"
    
    npm run dev
else
    echo "❌ 依赖安装失败"
    exit 1
fi
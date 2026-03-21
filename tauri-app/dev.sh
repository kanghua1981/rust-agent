#!/bin/bash
set -e

echo "🚀 启动 e_agent_gui 开发环境..."

# 检查是否安装了 Tauri CLI
if ! command -v tauri &> /dev/null; then
    echo "📦 安装 Tauri CLI..."
    npm install @tauri-apps/cli
fi

# 安装前端依赖
echo "📦 安装前端依赖..."
npm install

# 启动开发服务器
echo "🚀 启动开发服务器..."
echo "📱 前端开发服务器: http://localhost:9581"
echo "🖥️  e_agent_gui 应用将自动启动..."
npm run tauri dev
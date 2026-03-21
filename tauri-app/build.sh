#!/bin/bash
set -e

echo "🚀 开始构建 e_agent_gui 应用..."

## 从web-ui目录复制构建好的前端文件
echo "📂 复制前端文件..."
cp -fr ../web-ui/src/* src/
## 构建 Tauri 应用
echo "🔨 构建 Tauri 应用..."
# 检查是否安装了 Tauri CLI
if ! command -v tauri &> /dev/null; then
    echo "📦 安装 Tauri CLI..."
    npm install @tauri-apps/cli
fi

# 安装前端依赖
echo "📦 安装前端依赖..."
npm install

# 构建前端
echo "🔨 构建前端..."
npm run build

# 构建 Tauri 应用
echo "🔨 构建 Tauri 应用..."
npm run tauri build

echo "✅ 构建完成！应用位于：tauri-app/src-tauri/target/release/e_agent_gui"

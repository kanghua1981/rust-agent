#!/bin/bash
# Rust Agent Web UI 启动脚本
# 双击或运行此脚本即可启动 Web UI

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DIST_DIR="$SCRIPT_DIR/dist"
PORT=3000

# 颜色输出
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN}🚀 Rust Agent Web UI${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

# 检查 dist 目录
if [ ! -d "$DIST_DIR" ]; then
    echo "❌ dist/ 目录不存在，请先构建前端："
    echo "   npm run build"
    exit 1
fi

echo "📂 静态文件: $DIST_DIR"
echo "🌐 端口: $PORT"
echo ""

# 尝试不同的服务器
start_server() {
    cd "$DIST_DIR"
    
    # 优先使用 miniserve（最佳体验）
    if command -v miniserve >/dev/null 2>&1; then
        echo "✓ 使用 miniserve 服务器"
        echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
        echo ""
        echo -e "  浏览器访问: ${GREEN}http://localhost:$PORT${NC}"
        echo ""
        echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
        echo "按 Ctrl+C 停止服务"
        echo ""
        
        # 尝试自动打开浏览器
        sleep 1
        if command -v xdg-open >/dev/null 2>&1; then
            xdg-open "http://localhost:$PORT" 2>/dev/null || true
        fi
        
        miniserve . --index index.html -p "$PORT"
        return
    fi
    
    # 使用 Python 3
    if command -v python3 >/dev/null 2>&1; then
        echo "✓ 使用 Python HTTP 服务器"
        echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
        echo ""
        echo -e "  浏览器访问: ${GREEN}http://localhost:$PORT${NC}"
        echo ""
        echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
        echo "按 Ctrl+C 停止服务"
        echo ""
        
        # 尝试自动打开浏览器
        sleep 1
        if command -v xdg-open >/dev/null 2>&1; then
            xdg-open "http://localhost:$PORT" 2>/dev/null || true
        fi
        
        python3 -m http.server "$PORT"
        return
    fi
    
    # 使用 Node serve
    if command -v npx >/dev/null 2>&1; then
        echo "✓ 使用 Node serve"
        echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
        echo ""
        echo -e "  浏览器访问: ${GREEN}http://localhost:$PORT${NC}"
        echo ""
        echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
        npx serve -s . -p "$PORT"
        return
    fi
    
    # 都没有
    echo "❌ 未找到可用的 HTTP 服务器"
    echo ""
    echo "请安装以下任意一个："
    echo "  • miniserve:  cargo install miniserve"
    echo "  • Python 3:   apt install python3 / brew install python3"
    echo "  • Node.js:    apt install nodejs / brew install node"
    exit 1
}

# 捕获退出信号
trap 'echo ""; echo "👋 服务已停止"; exit 0' INT TERM

# 启动服务器
start_server

#!/bin/bash
# Rust Agent Web UI 部署脚本

set -e

VERSION="0.1.0"
BUILD_DIR="dist"
PACKAGE_NAME="rust-agent-ui-v${VERSION}"

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info() { echo -e "${GREEN}[INFO]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

# 检查依赖
check_deps() {
    command -v npm >/dev/null 2>&1 || error "npm 未安装"
    [ -f "package.json" ] || error "请在 web-ui 目录下运行此脚本"
}

# 构建前端
build() {
    info "开始构建前端..."
    npm run build || error "构建失败"
    
    if [ ! -d "$BUILD_DIR" ]; then
        error "构建目录 $BUILD_DIR 不存在"
    fi
    
    info "构建完成，输出目录: $BUILD_DIR"
    du -sh "$BUILD_DIR"
}

# 打包成压缩文件
package() {
    build
    
    info "打包静态文件..."
    
    # tar.gz 格式
    tar -czf "${PACKAGE_NAME}.tar.gz" -C "$BUILD_DIR" .
    info "已创建: ${PACKAGE_NAME}.tar.gz"
    
    # zip 格式（Windows 友好）
    if command -v zip >/dev/null 2>&1; then
        (cd "$BUILD_DIR" && zip -r "../${PACKAGE_NAME}.zip" .)
        info "已创建: ${PACKAGE_NAME}.zip"
    fi
    
    info "打包完成"
    ls -lh "${PACKAGE_NAME}".{tar.gz,zip} 2>/dev/null || true
}

# 构建 Docker 镜像
build_docker() {
    build
    
    if ! command -v docker >/dev/null 2>&1; then
        error "Docker 未安装"
    fi
    
    info "创建 Dockerfile..."
    cat > Dockerfile.tmp <<'EOF'
FROM nginx:alpine
COPY dist /usr/share/nginx/html
RUN echo 'server { \
    listen 80; \
    root /usr/share/nginx/html; \
    index index.html; \
    location / { try_files $uri $uri/ /index.html; } \
}' > /etc/nginx/conf.d/default.conf
EXPOSE 80
EOF
    
    info "构建 Docker 镜像..."
    docker build -f Dockerfile.tmp -t "rust-agent-ui:${VERSION}" -t "rust-agent-ui:latest" .
    rm Dockerfile.tmp
    
    info "Docker 镜像构建完成"
    docker images | grep rust-agent-ui
    
    echo ""
    info "运行容器: docker run -d -p 3000:80 rust-agent-ui:latest"
}

# 部署到远程服务器
deploy_remote() {
    local REMOTE=$1
    
    if [ -z "$REMOTE" ]; then
        error "请指定远程服务器: user@host:/path"
    fi
    
    build
    
    info "部署到远程服务器: $REMOTE"
    rsync -avz --delete "$BUILD_DIR/" "$REMOTE" || error "部署失败"
    
    info "部署完成"
}

# 启动本地预览
preview() {
    if [ ! -d "$BUILD_DIR" ]; then
        build
    fi
    
    info "启动本地预览服务器..."
    
    if command -v python3 >/dev/null 2>&1; then
        info "访问 http://localhost:8000"
        cd "$BUILD_DIR" && python3 -m http.server 8000
    elif command -v serve >/dev/null 2>&1; then
        info "访问 http://localhost:3000"
        serve -s "$BUILD_DIR" -p 3000
    else
        warn "未找到 python3 或 serve，使用 npm preview"
        npm run preview
    fi
}

# 打包可移植版本（包含服务器）
portable() {
    build
    
    info "创建可移植版本..."
    
    local PORTABLE_DIR="${PACKAGE_NAME}-portable"
    rm -rf "$PORTABLE_DIR"
    mkdir -p "$PORTABLE_DIR"
    
    # 复制静态文件
    cp -r "$BUILD_DIR" "$PORTABLE_DIR/"
    
    # 添加启动脚本
    cp start-ui.sh "$PORTABLE_DIR/"
    cp start-ui.bat "$PORTABLE_DIR/"
    chmod +x "$PORTABLE_DIR/start-ui.sh"
    
    # 创建 README
    cat > "$PORTABLE_DIR/README.txt" <<'EOF'
Rust Agent Web UI - 可移植版本

快速开始:

Linux/Mac:
  ./start-ui.sh

Windows:
  双击 start-ui.bat

或使用 Python 手动启动:
  cd dist
  python -m http.server 3000
  
然后访问 http://localhost:3000

服务器要求:
- Python 3 (推荐，通常已预装)
- 或 Node.js
- 或 miniserve (cargo install miniserve)
EOF
    
    # 打包
    info "压缩打包..."
    tar -czf "${PORTABLE_DIR}.tar.gz" "$PORTABLE_DIR"
    
    if command -v zip >/dev/null 2>&1; then
        zip -r "${PORTABLE_DIR}.zip" "$PORTABLE_DIR"
    fi
    
    info "可移植版本创建完成:"
    ls -lh "${PORTABLE_DIR}".{tar.gz,zip} 2>/dev/null || ls -lh "${PORTABLE_DIR}.tar.gz"
    
    echo ""
    info "用户使用方法:"
    echo "  1. 解压: tar -xzf ${PORTABLE_DIR}.tar.gz"
    echo "  2. 启动: cd ${PORTABLE_DIR} && ./start-ui.sh"
}

# 清理构建产物
clean() {
    info "清理构建产物..."
    rm -rf "$BUILD_DIR"
    rm -f "${PACKAGE_NAME}".{tar.gz,zip}
    rm -f Dockerfile.tmp
    info "清理完成"
}

# 显示帮助
show_help() {
    cat <<EOF
Rust Agent Web UI 部署脚本

用法: $0 [选项]

选项:
  -b, --build              仅构建（生成 dist/）
  -p, --package            构建并打包成 tar.gz/zip
  -d, --docker             构建 Docker 镜像
  -r, --remote <target>    部署到远程服务器（rsync）
      --preview            启动本地预览服务器
      --portable           打包可移植版本（含启动脚本）
  -c, --clean              清理构建产物
  -h, --help               显示此帮助

示例:
  $0 --build                      # 构建前端
  $0 --package                    # 打包成压缩文件
  $0 --portable                   # 打包可移植版本
  $0 --docker                     # 构建 Docker 镜像
  $0 --remote user@server:/www/   # 部署到服务器
  $0 --preview                    # 本地预览

本地服务/应用:
  ./start-ui.sh                   # Linux/Mac 双击启动
  start-ui.bat                    # Windows 双击启动
  sudo ./install-service.sh       # 安装为系统服务（Linux）
  install-service.ps1             # 安装为系统服务（Windows）

更多信息请查看:
  - DEPLOY.md         部署指南
  - LOCAL_SERVICE.md  本地服务指南
EOF
}

# 主逻辑
main() {
    check_deps
    
    case "${1:-}" in
        -b|--build)
            build
            ;;
        -p|--package)
            package
         -portable)
            portable
            ;;
        -   ;;
        -d|--docker)
            build_docker
            ;;
        -r|--remote)
            deploy_remote "$2"
            ;;
        --preview)
            preview
            ;;
        -c|--clean)
            clean
            ;;
        -h|--help|"")
            show_help
            ;;
        *)
            error "未知选项: $1\n使用 --help 查看帮助"
            ;;
    esac
}

main "$@"

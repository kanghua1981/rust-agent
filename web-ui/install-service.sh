#!/bin/bash
# 将 Rust Agent Web UI 安装为 Linux 系统服务
# 需要 sudo 权限

set -e

if [ "$EUID" -ne 0 ]; then 
    echo "请使用 sudo 运行此脚本"
    exit 1
fi

# 配置
SERVICE_NAME="rust-agent-ui"
INSTALL_DIR="/opt/rust-agent-ui"
SERVICE_PORT=3000
CURRENT_USER="${SUDO_USER:-$USER}"

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  安装 Rust Agent Web UI 系统服务"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "服务名称: $SERVICE_NAME"
echo "安装目录: $INSTALL_DIR"
echo "端口: $SERVICE_PORT"
echo "运行用户: $CURRENT_USER"
echo ""

read -p "确认安装？[y/N] " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "已取消安装"
    exit 0
fi

# 检查 dist 目录
if [ ! -d "dist" ]; then
    echo "❌ dist/ 目录不存在，请先构建："
    echo "   npm run build"
    exit 1
fi

# 创建安装目录
echo "📂 创建安装目录..."
mkdir -p "$INSTALL_DIR"
cp -r dist/* "$INSTALL_DIR/"
chown -R "$CURRENT_USER:$CURRENT_USER" "$INSTALL_DIR"

# 下载 miniserve（如果没有）
if ! command -v miniserve >/dev/null 2>&1; then
    echo "📥 下载 miniserve..."
    MINISERVE_URL="https://github.com/svenstaro/miniserve/releases/download/v0.24.0/miniserve-v0.24.0-x86_64-unknown-linux-musl"
    curl -L "$MINISERVE_URL" -o "$INSTALL_DIR/miniserve"
    chmod +x "$INSTALL_DIR/miniserve"
    EXEC_START="$INSTALL_DIR/miniserve . --index index.html -p $SERVICE_PORT"
else
    EXEC_START="$(which miniserve) $INSTALL_DIR --index index.html -p $SERVICE_PORT"
fi

# 创建 systemd 服务文件
echo "⚙️  创建 systemd 服务..."
cat > "/etc/systemd/system/${SERVICE_NAME}.service" <<EOF
[Unit]
Description=Rust Agent Web UI
Documentation=https://github.com/your-repo/rust_agent
After=network.target

[Service]
Type=simple
User=$CURRENT_USER
WorkingDirectory=$INSTALL_DIR
ExecStart=$EXEC_START
Restart=on-failure
RestartSec=5s

# 安全设置
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/tmp

# 日志
StandardOutput=journal
StandardError=journal
SyslogIdentifier=$SERVICE_NAME

[Install]
WantedBy=multi-user.target
EOF

# 重载 systemd
echo "🔄 重载 systemd..."
systemctl daemon-reload

# 启用并启动服务
echo "▶️  启动服务..."
systemctl enable "$SERVICE_NAME"
systemctl start "$SERVICE_NAME"

# 检查状态
sleep 1
if systemctl is-active --quiet "$SERVICE_NAME"; then
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "✅ 安装成功！"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    echo "服务状态: $(systemctl is-active $SERVICE_NAME)"
    echo "访问地址: http://localhost:$SERVICE_PORT"
    echo ""
    echo "常用命令:"
    echo "  查看状态: sudo systemctl status $SERVICE_NAME"
    echo "  停止服务: sudo systemctl stop $SERVICE_NAME"
    echo "  重启服务: sudo systemctl restart $SERVICE_NAME"
    echo "  查看日志: sudo journalctl -u $SERVICE_NAME -f"
    echo "  卸载服务: sudo systemctl disable --now $SERVICE_NAME"
    echo "           sudo rm /etc/systemd/system/${SERVICE_NAME}.service"
    echo "           sudo rm -rf $INSTALL_DIR"
    echo ""
else
    echo "❌ 服务启动失败，请查看日志："
    echo "   sudo journalctl -u $SERVICE_NAME -n 50"
    exit 1
fi

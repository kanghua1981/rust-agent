# 本地服务部署指南

将 Web UI 部署为本地系统服务或独立应用程序。

---

## 🖥️ 方案 1: 独立可执行文件（推荐）

### 使用 miniserve（Rust 静态服务器）

**一键安装 + 运行：**

```bash
# 1. 安装 miniserve（只需一次）
cargo install miniserve

# 2. 启动服务（自动打开浏览器）
cd web-ui/dist
miniserve . --index index.html -p 3000
```

**Windows 双击启动：**

创建 `start-ui.bat`:
```batch
@echo off
cd /d "%~dp0dist"
miniserve . --index index.html -p 3000
pause
```

**Linux/Mac 一键启动：**

创建 `start-ui.sh`:
```bash
#!/bin/bash
cd "$(dirname "$0")/dist"
miniserve . --index index.html -p 3000
```

### 使用内置 Python（无需安装）

**Windows**: 创建 `start-ui.bat`
```batch
@echo off
cd /d "%~dp0dist"
python -m http.server 3000
start http://localhost:3000
```

**Linux/Mac**: 创建 `start-ui.sh`
```bash
#!/bin/bash
cd "$(dirname "$0")/dist"
python3 -m http.server 3000 &
sleep 1
xdg-open http://localhost:3000  # Linux
# open http://localhost:3000    # Mac
```

---

## ⚙️ 方案 2: 系统服务（开机自启）

### Linux systemd 服务

创建 `/etc/systemd/system/rust-agent-ui.service`:

```ini
[Unit]
Description=Rust Agent Web UI
After=network.target

[Service]
Type=simple
User=your-username
WorkingDirectory=/path/to/rust_agent/web-ui/dist
ExecStart=/usr/bin/python3 -m http.server 3000
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
```

**启动服务：**
```bash
sudo systemctl daemon-reload
sudo systemctl enable rust-agent-ui
sudo systemctl start rust-agent-ui
sudo systemctl status rust-agent-ui
```

### Windows 服务（NSSM）

```bash
# 1. 下载 NSSM: https://nssm.cc/download
# 2. 安装服务
nssm install RustAgentUI "C:\Python3\python.exe" "-m http.server 3000"
nssm set RustAgentUI AppDirectory "C:\path\to\rust_agent\web-ui\dist"
nssm set RustAgentUI DisplayName "Rust Agent Web UI"
nssm set RustAgentUI Description "Rust Agent Web 界面服务"
nssm set RustAgentUI Start SERVICE_AUTO_START

# 3. 启动服务
nssm start RustAgentUI

# 管理服务
nssm stop RustAgentUI
nssm restart RustAgentUI
nssm remove RustAgentUI confirm
```

---

## 🎯 方案 3: 打包成单个可执行文件

### 方法 A: 使用 static-web-server（推荐）

```bash
# 1. 下载预编译二进制（无需编译）
# Linux
wget https://github.com/static-web-server/static-web-server/releases/download/v2.29.0/static-web-server-v2.29.0-x86_64-unknown-linux-gnu.tar.gz
tar -xzf static-web-server-*.tar.gz

# Windows
# 下载 .zip 并解压

# 2. 将二进制和 dist/ 打包在一起
mkdir rust-agent-ui-portable
cp static-web-server rust-agent-ui-portable/
cp -r dist rust-agent-ui-portable/

# 3. 创建启动脚本
cat > rust-agent-ui-portable/start.sh <<'EOF'
#!/bin/bash
./static-web-server -p 3000 -d ./dist -g trace
EOF
chmod +x rust-agent-ui-portable/start.sh

# 4. 打包发布
tar -czf rust-agent-ui-portable.tar.gz rust-agent-ui-portable/
```

**用户使用：**
```bash
tar -xzf rust-agent-ui-portable.tar.gz
cd rust-agent-ui-portable
./start.sh
```

### 方法 B: 将 Web UI 嵌入到 Rust Agent

**这是最优雅的方案！** 让 Rust Agent 直接提供 Web UI。

我会为你创建一个集成方案。

---

## 📱 方案 4: 桌面应用（Tauri）

将 Web UI 打包成原生桌面应用（<5MB）。

### 安装 Tauri

```bash
cd web-ui
npm install --save-dev @tauri-apps/cli
```

### 创建 Tauri 配置

创建 `src-tauri/tauri.conf.json`:
```json
{
  "build": {
    "beforeBuildCommand": "npm run build",
    "beforeDevCommand": "npm run dev",
    "devPath": "http://localhost:5173",
    "distDir": "../dist"
  },
  "package": {
    "productName": "Rust Agent",
    "version": "0.1.0"
  },
  "tauri": {
    "allowlist": {
      "all": false,
      "shell": {
        "all": false,
        "open": true
      }
    },
    "bundle": {
      "active": true,
      "identifier": "com.rustagent.app",
      "targets": "all",
      "icon": [
        "icons/32x32.png",
        "icons/128x128.png",
        "icons/icon.icns",
        "icons/icon.ico"
      ]
    },
    "windows": [
      {
        "title": "Rust Agent",
        "width": 1200,
        "height": 800,
        "resizable": true,
        "fullscreen": false
      }
    ]
  }
}
```

### 构建桌面应用

```bash
npx tauri build

# Windows: .msi / .exe
# Linux: .deb / .AppImage
# Mac: .dmg / .app
```

---

## 🚀 方案对比

| 方案 | 优点 | 缺点 | 适用场景 |
|------|------|------|----------|
| **独立可执行** | 简单、轻量 | 需要单独运行 | 快速测试、临时使用 |
| **系统服务** | 开机自启、后台运行 | 需要管理员权限 | 长期运行、服务器 |
| **单文件打包** | 零依赖、便携 | 包体积稍大 | 分发给他人 |
| **嵌入 Rust** | 集成最佳 | 需要重新编译 | 正式产品 |
| **Tauri 桌面** | 原生体验、体积小 | 需要额外配置 | 桌面应用产品 |

---

## 📦 一键安装脚本

我已创建 `install-service.sh`（Linux）和 `install-service.ps1`（Windows），
运行即可自动安装为系统服务。

---

## 🎁 推荐方案

**开发/个人使用**:
```bash
./start-ui.sh    # 双击启动
```

**服务器/长期运行**:
```bash
sudo systemctl enable rust-agent-ui
```

**分发给他人**:
```bash
./deploy.sh --portable    # 打包成单文件版本
```

**产品化**:
- 集成到 Rust Agent 主程序（无需单独启动）
- 或打包成 Tauri 桌面应用

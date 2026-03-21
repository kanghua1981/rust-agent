# Rust Agent Web UI

基于 WebSocket 的 Rust Agent 前端界面，支持远程代码协作。

## ✨ 功能特性

- 🗨️ **实时对话** - 流式响应，实时工具调用监控
- 📁 **文件浏览** - 浏览和管理工作目录文件
- 🛠️ **工具监控** - 实时查看工具执行状态和结果
- ⚙️ **配置管理** - 支持多组配置预设，快速切换环境
- 📊 **会话持久化** - 自动保存和恢复对话历史
- 🔄 **执行模式** - 自动/单层/计划/流水线多种执行模式
- ✅ **自动确认** - 可配置自动批准工具调用
- 🌙 **暗色主题** - 现代化 UI 设计

## 🚀 快速开始

### 开发模式

```bash
# 安装依赖
npm install

# 启动开发服务器（热重载）
npm run dev

# 访问 http://localhost:5173
```

### 生产构建

```bash
# 构建生产版本
npm run build

# 预览构建结果
npm run preview
```

## 📦 部署方式

### 一键启动（无需安装）

**Linux/Mac:**
```bash
./start-ui.sh
```

**Windows:**
```
双击 start-ui.bat
```

脚本会自动：
- 检测可用的服务器（miniserve / Python / Node serve）
- 在 http://localhost:3000 启动服务
- 自动打开浏览器

### 系统服务（开机自启）

**Linux 系统服务:**
```bash
sudo ./install-service.sh
sudo systemctl status rust-agent-ui
```

**Windows 服务:**
```powershell
# 以管理员身份运行
.\install-service.ps1

# 管理服务
Get-Service RustAgentUI
```

**详细本地服务文档**: 请查看 [LOCAL_SERVICE.md](./LOCAL_SERVICE.md)

### 自动化部署

```bash
# 查看所有选项
./deploy.sh --help

# 构建并打包成压缩文件
./deploy.sh --package

# 打包可移植版本（含启动脚本）
./deploy.sh --portable

# 构建 Docker 镜像
./deploy.sh --docker

# 部署到远程服务器
./deploy.sh --remote user@server:/www/

# 本地预览
./deploy.sh --preview
```

### 静态文件服务器

```bash
# Python
cd dist && python3 -m http.server 3000

# Node.js serve
npx serve -s dist -p 3000

# Nginx
# 使用提供的 nginx.conf 配置文件
```

### Docker 部署

```bash
docker build -t rust-agent-ui .
docker run -d -p 3000:80 rust-agent-ui
```

**完整部署文档**: 请查看 [DEPLOY.md](./DEPLOY.md)

## ⚙️ 配置

### 连接 Rust Agent 服务器

1. 启动 Rust Agent WebSocket 服务器：
   ```bash
   cd ..
   ./target/release/agent --mode server --host 0.0.0.0 --port 9527
   ```

2. 在 Web UI 设置页面配置：
   - **服务器地址**: ws://localhost:9527（或远程地址）
   - **工作目录**: /path/to/your/project
   - **模型**: claude-opus-4-5（可选）
   - **执行模式**: auto / simple / plan / pipeline

### 配置预设

Web UI 支持保存多组配置预设：

1. 进入 **设置** → **配置预设** 标签
2. 点击 **新建预设** 创建配置（开发环境、测试环境等）
3. 一键 **应用** 切换不同配置

所有配置自动保存到浏览器 localStorage，刷新后自动恢复。

## 🎯 部署方案选择

| 方案 | 适用场景 | 优势 | 启动方式 |
|------|---------|------|---------|
| **一键启动** | 本地开发/测试 | 无需配置，双击启动 | `./start-ui.sh` |
| **系统服务** | 个人工作站 | 开机自启，后台运行 | `sudo ./install-service.sh` |
| **可移植版** | 分发给他人 | 解压即用，跨平台 | `./deploy.sh --portable` |
| **Docker** | 生产环境/云服务器 | 隔离环境，易于迁移 | `./deploy.sh --docker` |
| **Nginx/Caddy** | 企业生产环境 | 高性能，支持 HTTPS | 见 [DEPLOY.md](./DEPLOY.md) |
| **CDN 托管** | 公开访问 | 全球加速，零运维 | Vercel/Netlify 一键部署 |

**推荐方案**:
- 🖥️ **个人使用**: 系统服务（`install-service.sh/ps1`）
- 👥 **团队分发**: 可移植版（`deploy.sh --portable`）
- ☁️ **云部署**: Docker + Nginx
- 🌐 **公开访问**: CDN 托管（Vercel/Netlify）

## 📁 项目结构

```
web-ui/
├── src/
│   ├── components/     # React 组件
│   │   ├── Chat.tsx    # 聊天界面
│   │   ├── FileBrowser.tsx # 文件浏览器
│   │   ├── ToolMonitor.tsx # 工具监控
│   │   └── Settings.tsx    # 设置面板
│   ├── hooks/         # 自定义 Hook
│   │   ├── useWebSocket.ts # WebSocket 连接
│   │   └── useAgent.ts     # Agent 状态管理
│   ├── stores/        # 状态管理
│   │   └── agentStore.ts   # Agent 状态存储
│   ├── types/         # TypeScript 类型定义
│   │   └── agent.ts       # Agent 协议类型
│   └── App.tsx        # 主应用组件
├── public/            # 静态资源
└── package.json       # 项目配置
```

## 协议说明

Web UI 与 Rust Agent 通过 WebSocket 通信，使用 JSON 事件协议：

### 客户端 → 服务器
```json
{
  "type": "user_message",
  "data": {
    "text": "帮我重构 main.rs",
    "workdir": "/path/to/project"  // 可选
  }
}
```

### 服务器 → 客户端
```json
{
  "type": "streaming_token",
  "data": {"token": "Hello"}
}
```

完整事件类型参考 Rust Agent 文档。

## 开发计划

- [ ] 基础聊天界面
- [ ] 文件浏览器集成
- [ ] 工具调用确认面板
- [ ] 工作目录管理
- [ ] 模型配置界面
- [ ] 会话历史保存
- [ ] 代码高亮显示
- [ ] 暗色/亮色主题
- [ ] 移动端适配


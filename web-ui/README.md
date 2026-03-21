# Rust Agent Web UI

基于 WebSocket 的 Rust Agent 前端界面，支持远程代码协作。

## 功能特性

- 🗨️ 实时聊天界面
- 📁 文件浏览器
- 🛠️ 工具调用监控
- ⚙️ 工作目录设置
- 🔧 模型配置
- 📊 会话历史管理

## 技术栈

- React 18 + TypeScript
- Vite 构建工具
- Tailwind CSS
- WebSocket (原生)
- Zustand 状态管理

## 快速开始

```bash
# 安装依赖
npm install

# 启动开发服务器
npm run dev

# 构建生产版本
npm run build
```

## 配置

1. 启动 Rust Agent WebSocket 服务器：
   ```bash
   ./target/release/agent --mode server --host 0.0.0.0 --port 9527
   ```

2. 在 Web UI 中配置连接：
   - 服务器地址：ws://localhost:9527
   - 工作目录：/path/to/your/project

## 项目结构

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


# WeChat Bridge for Rust Agent

将微信消息桥接到 [Rust Agent](https://github.com/xxx/rust-agent) 的 WebSocket Server。

## 架构

```
微信用户 ←→ wechatbot.dev 长轮询 ←→ @wechatbot/wechatbot SDK
                                        │
                                   WeChat Bridge
                                        │
                              WebSocket JSON 协议
                                        │
                              Rust Agent Server
                              (./agent --mode server)
```

- 每个微信用户独立一个 Agent WebSocket 连接（对应 Server 侧一个 worker 进程）
- 支持聚合模式（默认，Agent 完成回复后一次性发送）和分片模式（流式发送）
- 会话持久化：同一微信用户的多轮对话共享一个 Agent session
- TTL 自动过期：30 分钟无消息自动释放连接

## 快速开始

### 1. 启动 Agent Server

```bash
cd /path/to/your/project
./agent --mode server --port 9527
```

### 2. 安装依赖

```bash
cd bridges/wechat
npm install
```

### 3. 启动 Bridge

```bash
# 聚合模式（默认，推荐）
AGENT_URL=ws://localhost:9527/agent npx tsx src/main.ts

# 分片模式（每 200 字符发送一条消息）
CHUNK_SIZE=200 AGENT_URL=ws://localhost:9527/agent npx tsx src/main.ts
```

### 4. 扫码登录

启动后终端会显示二维码，用微信扫描即可登录。登录凭证会持久化到 `~/.wechatbot/`，下次启动无需重新扫码。

## 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `AGENT_URL` | `ws://localhost:9527/agent` | Agent Server WebSocket 地址 |
| `CHUNK_SIZE` | `0`（聚合模式） | 分片发送阈值，0 表示等 done 后一次性发送 |
| `SESSION_TTL_MS` | `1800000`（30 分钟） | 会话超时毫秒数 |
| `LOG_LEVEL` | `info` | 日志级别：debug / info / warn / error / silent |
| `STORAGE_DIR` | `~/.wechatbot` | wechatbot 存储目录（登录凭证） |

## 项目结构

```
bridges/wechat/
├── package.json
├── tsconfig.json
├── src/
│   ├── main.ts             # CLI 入口
│   ├── config.ts           # 配置管理
│   ├── protocol.ts         # Agent WebSocket 协议类型
│   ├── agent-client.ts     # Agent Server WebSocket 客户端
│   ├── session-pool.ts     # WeChat 用户连接池
│   └── gateway.ts          # 核心桥接逻辑
└── README.md
```

## 发送模式

### 聚合模式（默认，`CHUNK_SIZE=0`）

Agent 完成完整回复后，一次性发送给微信用户。避免刷屏，对话体验更自然。

### 分片模式（`CHUNK_SIZE=200`）

每积攒 N 个字符就发送一条消息，给用户"正在输入"的感觉。适合 Agent 生成长文本的场景。

## 安全

- 默认自动批准所有工具确认请求（微信场景下用户无法实时确认）
- 如需更安全的策略，可修改 `gateway.ts` 中的 `confirm_required` 处理逻辑：
  - 改为向用户发送确认消息询问
  - 或限制可自动执行的工具白名单

## 限制

- 仅处理微信文本消息（图片/语音/文件等暂不支持）
- 微信不支持真正的流式输出，分片模式是通过多条消息模拟
- 需要 Node.js ≥ 22

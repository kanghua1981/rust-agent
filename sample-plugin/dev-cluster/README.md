# dev-cluster 插件

多节点开发集群插件，作为「workspaces + MCP」功能的完整范例。

## 安装

```bash
cp -r sample/dev-cluster .agent/plugins/dev-cluster
```

根据实际环境修改以下文件后重启 Agent 或执行 `/plugins reload`：
- `workspaces.toml` — 改成你实际的目录和 IP
- `mcp/filesystem.toml` — 改成你授权的目录
- `mcp/multi-server.toml` — 填入真实的 API Token

## 目录结构

```
dev-cluster/
├── plugin.toml                        # 插件元数据
├── workspaces.toml                    # 多节点拓扑（nodes + peers）
├── README.md
│
├── mcp/                               # MCP Server 配置（每个 .toml = 一套服务）
│   ├── filesystem.toml                # stdio 传输（npx 按需启动）
│   ├── brave-search.toml              # HTTP/SSE 传输（连接已运行的服务）
│   └── multi-server.toml              # 多 server 格式（[[server]] 数组）
│
├── tools/
│   ├── probe_nodes.json               # 探测节点可达性
│   └── probe_nodes.sh
│
├── skills/
│   └── multi-node-guide.md            # 多节点路由 + MCP 工具使用指南
│
└── hooks/
    ├── probe_on_start.toml            # agent.start blocking — 启动时探测 peer
    └── scripts/
        └── probe_on_start.sh
```

## workspaces.toml

定义本机节点（`[[node]]`）和远程 peer（`[[peer]]`）：

```toml
[[node]]
name      = "backend"
workdir   = "/workspace/backend"
isolation = "sandbox"
exec_mode = "plan"              # 强制使用 plan+execute 模式
tags      = ["backend", "rust"]

[[peer]]
name = "gpu-box"
url  = "ws://192.168.1.100:9527"
```

**加载机制**：Agent 启动时 `PluginManager.collect_workspace()` 合并所有已启用插件的 `workspaces.toml`，注入到全局拓扑，无需手动修改项目根目录的 `.agent/workspaces.toml`。

**相对路径**：`workdir` 若为相对路径，自动以插件目录为基准展开。

## MCP 配置（`mcp/` 目录）

### 单 server 格式（推荐，每文件一个服务）

**stdio 传输**（本地进程，Agent 自动 spawn）：
```toml
# mcp/my-tool.toml
name    = "my-tool"
command = "npx"
args    = ["-y", "@modelcontextprotocol/server-my-tool", "/allowed/dir"]
[env]
API_KEY = "secret"
```

**HTTP/SSE 传输**（连接已运行的远程/本地服务）：
```toml
# mcp/remote-service.toml
name = "remote"
url  = "http://localhost:8080"    # 自动追加 /sse
[headers]
Authorization = "Bearer <token>"
```

### 多 server 格式（一文件多服务）

```toml
# mcp/batch.toml
[[server]]
name    = "github"
command = "npx"
args    = ["-y", "@modelcontextprotocol/server-github"]
[server.env]
GITHUB_PERSONAL_ACCESS_TOKEN = "ghp_xxx"

[[server]]
name = "postgres"
command = "npx"
args    = ["-y", "@modelcontextprotocol/server-postgres", "postgresql://localhost/db"]
```

**加载机制**：Agent 启动时扫描所有启用插件的 `mcp/` 目录，依次连接每个 MCP Server，工具名前缀 = `name` 字段。连接失败只打警告，不阻断 Agent 启动。

## 常用 MCP Server

| 包名 | 传输 | 提供工具前缀 |
|------|------|-------------|
| `@modelcontextprotocol/server-filesystem` | stdio | `filesystem__` |
| `@modelcontextprotocol/server-github` | stdio | `github__` |
| `@modelcontextprotocol/server-brave-search` | stdio/HTTP | `brave__` |
| `@modelcontextprotocol/server-postgres` | stdio | `postgres__` |
| `@modelcontextprotocol/server-puppeteer` | stdio | `puppeteer__` |
| `@modelcontextprotocol/server-slack` | stdio | `slack__` |

## Hook 说明

### `probe_on_start` — blocking
Agent 启动时（`agent.start`）探测所有 peer 节点 `/health` 接口，将结果写入
`/tmp/dev-cluster-nodes.json`。

`blocking` 模式确保探测在接受第一条用户消息前完成，LLM 可以立刻知道哪些节点在线。

> 如果 peer 都在本机，可改为 `mode = "fire_and_forget"` 让启动更快。

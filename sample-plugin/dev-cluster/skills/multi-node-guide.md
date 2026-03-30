# 多节点 + MCP 工作流技能

本技能说明如何在多节点集群中路由任务，以及如何使用 MCP 工具。

## 集群拓扑（由 dev-cluster 插件的 workspaces.toml 注册）

| 节点 | 目录 | 用途 | 执行模式 |
|------|------|------|---------|
| `frontend` | /workspace/frontend | React/TS 前端 | auto |
| `backend`  | /workspace/backend  | Rust/Python 后端 | plan |
| `infra`    | /workspace/infra    | Terraform/K8s | pipeline |
| `gpu-worker@gpu-box` | 远程 GPU 机 | 模型训练/推理 | — |
| `*@ci-runner` | 远程 CI 机 | 测试/构建 | — |

## 节点路由

### 基本路由
```
# 路由到本地 frontend 节点
call_node(node="frontend", task="修复登录页面的 CSS 对齐问题")

# 路由到本地 backend 节点
call_node(node="backend", task="新增 /api/v2/users 接口")

# 路由到远程 GPU 节点（需要 server 模式 + peer 握手）
call_node(node="gpu-worker@gpu-box", task="运行模型微调脚本")
```

### 跨节点任务
```
# 先在 backend 完成接口，再在 frontend 对接
call_node(node="backend",  task="新增 GET /api/stats 接口，返回项目统计 JSON")
call_node(node="frontend", task="在 Dashboard 页面调用 GET /api/stats 并渲染图表")
```

## MCP 工具使用

### filesystem MCP（stdio，由 npx 自动启动）
```
# 读取文件（工具名 = "filesystem" + "__" + 原工具名）
filesystem__read_file(path="/workspace/backend/src/main.rs")

# 搜索文件
filesystem__search_files(path="/workspace/frontend/src", pattern="*.tsx")
```

### brave MCP（HTTP/SSE，需本地先启动服务）
```
# 联网搜索
brave__brave_web_search(query="rust tokio async best practices 2024")
```

### github MCP
```
# 查看 PR 列表
github__list_pull_requests(owner="yourname", repo="yourproject")

# 创建 issue
github__create_issue(owner="yourname", repo="yourproject",
  title="Bug: login page crash", body="...")
```

## 注意事项

1. **stdio MCP**：Agent 启动时自动 spawn 子进程，连接失败只打警告，不阻断启动。
2. **HTTP MCP**：需要目标服务已在运行，否则连接失败会跳过该服务器。
3. **peer 节点**：仅在 Agent 以 `--mode server` 启动时才会主动探测，CLI 模式不探测 peer。
4. **infra 节点**强制 `pipeline` 模式，所有变更都经过 Checker 验证，防止误操作生产资源。

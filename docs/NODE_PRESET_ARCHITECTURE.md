# 架构设计：Node、Preset、Peer 三层

> 版本: 2.0 | 日期: 2026-06-24

## 概念定义

```
┌──────────────────────────────────────────────────────────────────────────┐
│                                                                          │
│   Node（节点）               Preset（预设）           Peer（远端）        │
│   ────────────              ────────────             ────────────        │
│   服务器声明的工作区          用户保存的连接配置        远程 Agent 服务器     │
│   回答"这台机器有什么项目"    回答"我想怎么连接到服务器"   回答"集群里还有谁"  │
│                                                                          │
│   存储: global.db.nodes     存储: global.db.presets    存储: global.db.peers│
│   管理: WebSocket CRUD      管理: WebSocket CRUD       管理: WebSocket CRUD│
│   创建: 管理员               创建: 用户                 创建: 管理员        │
│   作用域: 单服务器           作用域: 跨服务器            作用域: 跨服务器    │
│   受众:  LLM + 用户          受众:  用户                受众:  系统         │
│                                                                          │
└──────────────────────────────────────────────────────────────────────────┘
```

## 统一存储：`global.db`

> **`workspaces.toml` 已完全废弃。** 所有 Node / Preset / Peer 数据均存储在 SQLite
> `global.db` 中。启动时仅做一次 legacy 种子导入（如果 DB 为空且存在
> `.agent/workspaces.toml`），之后不再读取该文件。

```
~/.config/rust_agent/global.db

  ┌─────────────────────────────────────────────────────────────────┐
  │ 表                    │ 用途                        │ 管理方式    │
  ├─────────────────────────────────────────────────────────────────┤
  │ nodes                 │ 本机工作区声明               │ WebSocket   │
  │ peers                 │ 远程 Agent 服务器发现配置     │ WebSocket   │
  │ presets               │ 用户连接预设（权威来源）       │ WebSocket   │
  │ workflows             │ 工作流定义                   │ WebSocket   │
  │ workflow_stages       │ 工作流阶段                   │ WebSocket   │
  │ workflow_runs         │ 工作流执行记录               │ WebSocket   │
  │ stage_results         │ 阶段执行结果                 │ WebSocket   │
  │ user_preferences      │ 用户偏好                     │ 预留        │
  │ endpoints             │ LLM API 端点                │ 预留        │
  │ _migrations           │ Schema 版本跟踪             │ 自动        │
  └─────────────────────────────────────────────────────────────────┘

客户端 localStorage（非权威，离线缓存）

  ├─ presets            ← 从服务端 presets_list 同步，server wins
  ├─ connectionHistory  ← 纯客户端
  └─ config             ← 纯客户端
```

## Node 的唯一来源：`global.db`

```
┌──────────────────────────────────────────────────────────────────┐
│ virtual_nodes 组装流程:                                           │
│                                                                  │
│ ① global.db.nodes  ← 启动时 load_vnodes() 读取全部               │
│ ② peer 探活结果     ← 后台定时探测 peer，名字带 @server 后缀       │
│                                                                  │
│ 合并后 → ready 帧发送给客户端                                      │
│                                                                  │
│ 注意：Node CRUD 变更后立即调用 load_vnodes() 刷新缓存               │
└──────────────────────────────────────────────────────────────────┘
```

## Node 与 Preset 的关系：`nodeRef`

Preset 通过 `nodeRef` 字段引用一个 Node：

```
Preset {
  name:         "GPU 训练"
  serverUrl:    "ws://gpu-box:9527"   ← 连哪台服务器
  nodeRef:      "abc123"              ← 引用 Node 的 id（可选）
  workdir:      "/data/ml"            ← 裸路径（nodeRef 未解析时使用）
  model:        "claude-sonnet"       ← AI 偏好
  autoApprove:  false                 ← 工具确认策略
  agentMode:    "auto"                ← 执行模式
  isolation:    "container"           ← nodeRef 解析后被覆盖
  newSession:   true                  ← 对话持久化
}
     │
     │ applyPreset(id) 或 ConnectModal 选择 Preset
     ▼
  nodeList 中查找 nodeRef → 继承:
    workdir   ← node.workdir    (覆盖 preset.workdir)
    isolation ← node.isolation  (覆盖 preset.isolation)
    exec_mode ← node.exec_mode  (覆盖 preset.agentMode)
```

### 解析逻辑（agentStore.applyPreset）

```
if preset.nodeRef && nodeList.length > 0:
    node = nodeList.find(n => n.id === preset.nodeRef)
    if node:
        workdir   = node.workdir
        isolation = node.isolation ?? (node.sandbox ? 'sandbox' : 'container')
        exec_mode = node.exec_mode || preset.agentMode  # Node 优先
    else:
        使用 preset 自身的字段
else:
    使用 preset 自身的字段
```

## 字段对比

| 字段 | Node | Preset | Peer | 说明 |
|------|------|--------|------|------|
| name | ✅ | ✅ | ✅ | 各自定义，不冲突 |
| serverUrl | ❌ | ✅ | ✅ | Node 隐含为本机 |
| url | ❌ | ❌ | ✅ | Peer 的 WebSocket 地址 |
| workdir | ✅ | ✅(可选) | ❌ | Node 是权威来源 |
| description | ✅ | ❌ | ❌ | 只有 Node 有 |
| isolation | ✅ | ✅(被覆盖) | ❌ | nodeRef 解析后以 Node 为准 |
| exec_mode | ✅ | ❌ | ❌ | Node 声明，通过 nodeRef 生效 |
| sandbox | ✅ | ❌ | ❌ | 遗留兼容字段 |
| tags | ✅ | ✅(可选) | ✅ | Node 用于 LLM 路由，Preset 用于 UI 分类 |
| token | ❌ | ❌ | ✅(可选) | Peer 认证 token |
| enabled | ❌ | ❌ | ✅ | 禁用后停止探测 |
| model | ❌ | ✅ | ❌ | 只有 Preset 有 |
| autoApprove | ❌ | ✅ | ❌ | 只有 Preset 有 |
| agentMode | ❌ | ✅ | ❌ | LLM 执行模式偏好 |
| newSession | ❌ | ✅ | ❌ | 对话持久化策略 |
| icon/color/sortOrder | ❌ | ✅ | ❌ | UI 装饰 |

## WebSocket API

### Node CRUD（客户端 → 服务端）

```
list_nodes     → 返回 merged virtual_nodes
add_node       → 写入 global.db.nodes，返回 node_saved
update_node    → 更新 global.db.nodes（按 id），返回 node_saved
delete_node    → 从 global.db.nodes 删除，返回 node_deleted
```

### Peer CRUD（客户端 → 服务端）

```
list_peers     → 返回 peers 列表
add_peer       → 写入 global.db.peers，返回 peer_saved
update_peer    → 更新 global.db.peers（按 id），返回 peer_saved
delete_peer    → 从 global.db.peers 删除，返回 peer_deleted
```

### Preset CRUD（客户端 → 服务端）

```
list_presets   → 返回 presets 列表
save_preset    → 写入 global.db.presets（upsert），返回 preset_saved
delete_preset  → 从 global.db.presets 删除，返回 preset_deleted
```

### 服务端 → 客户端事件

```
ready           → { virtual_nodes: [...], caps: {...}, ... }
nodes_list      → { nodes: [...] }
node_saved      → { node: {...} }
node_deleted    → { id: "..." }
peers_list      → { peers: [...] }
peer_saved      → { peer: {...} }
peer_deleted    → { id: "..." }
presets_list    → { presets: [...] }
preset_saved    → { preset: {...} }
preset_deleted  → { id: "..." }
```

## 从 workspaces.toml 的迁移

| 旧方案 (workspaces.toml) | 新方案 (global.db) |
|--------------------------|-------------------|
| 文件系统 `.agent/workspaces.toml` | SQLite `global.db` |
| 手动编辑 TOML | Web UI 管理面板 |
| Node / Peer 同文件混存 | 独立 `nodes` + `peers` 表 |
| 无版本控制 | `_migrations` 自动迁移 |
| 无并发控制 | WAL 模式 + busy_timeout |
| 无时间戳 | `created_at` / `updated_at` |
| 插件 workspaces.toml 合并 | 插件系统保留独立机制 |

### 遗留种子导入（仅首次启动）

```
if global.db.nodes 为空 && .agent/workspaces.toml 存在:
    导入 [[node]] 条目到 global.db.nodes
    记录日志 "Seeded N node(s) from legacy .agent/workspaces.toml"
```

此逻辑仅用于向后兼容，不影响新部署。

# 架构设计：Node 与 Preset 分层

> 版本: 1.0 | 日期: 2025-06-24

## 概念定义

```
┌──────────────────────────────────────────────────────────────────┐
│                                                                  │
│   Node（节点）                       Preset（预设）               │
│   ────────────                      ────────────                 │
│   服务器声明的工作区                  用户保存的连接配置            │
│   回答"这台服务器有什么项目"          回答"我想怎么连到某台服务器"   │
│                                                                  │
│   数据归属: 服务器                   数据归属: 客户端               │
│   存储位置: global.db + toml         存储位置: localStorage        │
│   创建者:   管理员                    创建者:   用户                │
│   作用域:   单服务器                  作用域:   跨服务器            │
│   受众:     LLM + 用户               受众:     用户                │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

## 分层存储模型

```
客户端 (浏览器)                    服务端 (每台机器)
───────────────                    ──────────────

localStorage                       workspaces.toml (引导层)
  ├─ presets ✅                      ├─ [cluster].token
  ├─ connectionHistory ✅            ├─ [[peer]] 远程节点声明
  └─ config ✅                       └─ [[node]] 初始工作区 (只读)

                                   global.db (运行时)
                                     ├─ nodes ✅          (动态管理)
                                     ├─ workflows ✅      (工作流)
                                     ├─ workflow_stages ✅
                                     ├─ workflow_runs ✅
                                     ├─ stage_results ✅
                                     └─ user_preferences ✅
```

## Node 的三层来源

```
┌──────────────────────────────────────────────────────────────────┐
│ 合并顺序 (后面的覆盖前面):                                        │
│                                                                  │
│ ① workspaces.toml [[node]]     ← bootstrap, 服务器启动即加载      │
│ ② global.db.nodes              ← 运行时动态管理, API 可读写       │
│ ③ peer 探活的 virtual_nodes    ← 启动时探活, 名字带 @server 后缀  │
│                                                                  │
│ 最终 merged → 发送给客户端的 virtual_nodes (ready 事件)            │
└──────────────────────────────────────────────────────────────────┘
```

## Node 与 Preset 的关系

Preset 可以**引用**一个 Node：

```
Preset {
  name:         "GPU 训练"
  serverUrl:    "ws://gpu-box:9527"   ← 连哪台服务器
  nodeRef:      "training"            ← 引用服务器的 Node (可选)
  workdir:      "/data/ml"            ← 裸路径 (没有 NodeRef 时用)
  model:        "claude-sonnet"       ← AI 偏好
  autoApprove:  false                 ← 工具确认策略
  agentMode:    "auto"                ← 执行模式
  newSession:   true                  ← 对话持久化
}
     │
     │ 连接后
     ▼
  virtual_nodes 中找到 nodeRef="training" → 继承:
    workdir, isolation, exec_mode, tags
```

## 字段对比

| 字段 | Node | Preset | 说明 |
|------|------|--------|------|
| name | ✅ | ✅ | 各自定义，不冲突 |
| serverUrl | ❌ | ✅ | Node 隐含为本机，Preset 指向任意服务器 |
| workdir | ✅ | ✅(可选) | Node 是权威来源 |
| description | ✅ | ❌ | 只有 Node 有 |
| isolation | ✅ | ❌ | Node 声明，Preset 不支持覆盖 |
| exec_mode | ✅ | ❌ | Node 声明 |
| tags | ✅ | ✅(可选) | Node 用于 LLM 路由，Preset 用于UI分类 |
| model | ❌ | ✅ | 只有 Preset 有 |
| autoApprove | ❌ | ✅ | 只有 Preset 有 |
| agentMode | ❌ | ✅ | LLM 执行模式偏好 |
| newSession | ❌ | ✅ | 对话持久化策略 |
| icon/color/sortOrder | ❌ | ✅ | UI 装饰 |

## API 设计

```
客户端 → 服务端:

  list_nodes     → 返回 merged virtual_nodes
  add_node       → 写入 global.db.nodes
  update_node    → 更新 global.db.nodes (按 id)
  delete_node    → 从 global.db.nodes 删除

服务端 → 客户端:

  ready          → { virtual_nodes: [...], caps: {...}, ... }
  nodes_list     → { nodes: [...] }             (响应 list_nodes)
  node_saved     → { node: {...} }              (确认 add/update)
  node_deleted   → { id: "..." }                (确认 delete)
```

## Preset 管理

- 预设**仅存 localStorage**，通过 zustand persist 自动持久化
- 连接时不拉取服务端预设 (不调 `list_presets`)
- `save_preset`/`delete_preset` 只写 localStorage
- 服务端 `presets` 表保留但不再主动使用
- 以后可加"保存 Node 为 Preset"的桥接按钮

## 迁移路径

1. 新增 `nodes` 表 (migration 002)
2. Node CRUD handlers
3. 合并 toml + DB nodes 到 virtual_nodes
4. 前端断开 `list_presets` → `presets_list` 的覆盖链路
5. 前端 Preset 改为纯 localStorage
6. (以后) 预设表迁移或删除

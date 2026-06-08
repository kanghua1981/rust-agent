# 全局数据库 & 编排引擎 设计文档

> 状态: 设计阶段 | 日期: 2025-06-22

---

## 1. 动机

### 1.1 现状问题

| 存储 | 位置 | 范围 | 问题 |
|------|------|------|------|
| `models.toml` | `~/.config/rust_agent/` | 全局 | 非结构化，无查询能力 |
| `memory.md` / `intelligent.json` | `.agent/` | 按项目 | 不跨项目 |
| Presets / 连接历史 | 浏览器 `localStorage` | 按浏览器 | 换浏览器丢失，无跨设备同步 |
| 编排/工作流 | **不存在** | — | Preset 相互独立，无法协作 |

### 1.2 目标

1. **全局持久化**：Preset、Workflow、执行历史不再依赖浏览器 localStorage
2. **跨项目编排**：多个 Preset 能编排为工作流，按序/并行执行
3. **多进程安全**：server 模式下每个连接 fork 一个 worker，多 worker 能安全共享 DB

---

## 2. 架构概览

```
~/.config/rust_agent/
├── models.toml              # 保持兼容（只读）
├── global.db                # ★ SQLite 全局数据库
│   ├── presets              # 预设配置（从 localStorage 迁移）
│   ├── workflows            # 编排工作流定义
│   ├── workflow_stages      # 工作流阶段（节点+边）
│   ├── workflow_runs        # 执行历史
│   ├── stage_results        # 阶段执行结果
│   └── _migrations          # 迁移版本记录
└── ...

项目目录/
└── .agent/
    ├── memory.md            # 保持不变
    └── intelligent.json     # 保持不变
```

```
┌──────────────────────────────────────────────┐
│                  agent server                 │
│                                              │
│  ┌────────┐  ┌────────┐  ┌────────┐         │
│  │worker 1│  │worker 2│  │worker 3│  ...    │
│  │ (fork) │  │ (fork) │  │ (fork) │         │
│  └───┬────┘  └───┬────┘  └───┬────┘         │
│      │           │           │               │
│      └───────────┼───────────┘               │
│                  │                           │
│           ┌──────▼──────┐                    │
│           │   db.rs     │  WAL + busy_timeout│
│           │  (SQLite)   │  multi-process safe│
│           └──────┬──────┘                    │
│                  │                           │
│           ┌──────▼──────┐                    │
│           │  global.db  │                    │
│           └─────────────┘                    │
└──────────────────────────────────────────────┘
```

---

## 3. 多进程安全性

### 3.1 问题

Server 模式下每个 WebSocket 连接 fork 一个独立 worker 进程。多个 worker
可能同时写 `global.db`。

### 3.2 解决方案：WAL + busy_timeout

```rust
// 每个连接初始化时执行
conn.execute_batch("
    PRAGMA journal_mode = WAL;
    PRAGMA busy_timeout = 5000;
    PRAGMA foreign_keys = ON;
    PRAGMA synchronous = NORMAL;
")?;
```

| 机制 | 作用 |
|------|------|
| **WAL 模式** | 读不阻塞写，写不阻塞读。多个读 + 一个写可并发 |
| **busy_timeout = 5000ms** | 两个写冲突时，后者等待最多 5 秒而非立即报错 |
| **synchronous = NORMAL** | 平衡安全性和性能（WAL 下 crash-safe） |
| **foreign_keys = ON** | 级联删除等约束由 DB 保证 |

### 3.3 写操作频率分析

| 操作 | 触发者 | 频率 | 事务耗时 |
|------|--------|------|----------|
| Preset CRUD | Web UI → Worker → DB | 用户手动，极罕见 | <1ms |
| Workflow CRUD | Web UI → Worker → DB | 用户手动，极罕见 | <1ms |
| WorkflowRun INSERT | Orchestrator | 每次编排 1 次 | <1ms |
| StageResult UPDATE | Orchestrator | 每阶段几次 | <1ms |

**结论：** 写操作都是单行/少数行的微事务。两个 worker 同时写锁冲突的概率极低；
即使冲突，busy_timeout 能保证等待后成功提交。

### 3.4 防御性封装

```rust
// db.rs — 所有写操作通过此函数，自动重试 SQLITE_BUSY
pub fn with_retry<T>(f: impl Fn() -> rusqlite::Result<T>) -> rusqlite::Result<T> {
    const MAX_RETRIES: u32 = 3;
    let mut attempts = 0;
    loop {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) if e == rusqlite::Error::SqliteFailure(
                ffi::Error { code: ffi::ErrorCode::DatabaseBusy, .. }, _
            ) && attempts < MAX_RETRIES => {
                attempts += 1;
                std::thread::sleep(Duration::from_millis(200 * attempts as u64));
            }
            Err(e) => return Err(e),
        }
    }
}
```

---

## 4. 数据库 Schema

### 4.1 完整 DDL

```sql
-- ═══════════════════════════════════════════════════════════════
-- 迁移版本管理
-- ═══════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS _migrations (
    version     INTEGER PRIMARY KEY,
    applied_at  TEXT NOT NULL DEFAULT (datetime('now')),
    description TEXT
);

-- ═══════════════════════════════════════════════════════════════
-- 预设配置（替代 localStorage presets）
-- ═══════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS presets (
    id              TEXT PRIMARY KEY,           -- UUID
    name            TEXT NOT NULL,              -- 显示名称
    server_url      TEXT NOT NULL,              -- ws://host:port
    workdir         TEXT,                       -- 默认工作目录
    model           TEXT,                       -- 模型别名 (models.toml key)
    auto_approve    INTEGER NOT NULL DEFAULT 0, -- 0/1
    agent_mode      TEXT NOT NULL DEFAULT 'auto',  -- auto|simple|plan|pipeline
    isolation       TEXT NOT NULL DEFAULT 'container', -- normal|container|sandbox
    new_session     INTEGER NOT NULL DEFAULT 0, -- 连接时是否新建会话
    icon            TEXT DEFAULT '🔧',          -- emoji 图标
    color           TEXT,                       -- CSS 颜色 (可选)
    tags            TEXT DEFAULT '[]',          -- JSON 数组 ["frontend","backend"]
    sort_order      INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_presets_name ON presets(name);
CREATE INDEX idx_presets_tags ON presets(tags);

-- ═══════════════════════════════════════════════════════════════
-- 编排工作流定义
-- ═══════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS workflows (
    id              TEXT PRIMARY KEY,           -- UUID
    name            TEXT NOT NULL,
    description     TEXT DEFAULT '',
    enabled         INTEGER NOT NULL DEFAULT 1,
    default_timeout INTEGER NOT NULL DEFAULT 600,  -- 默认阶段超时 (秒)
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ═══════════════════════════════════════════════════════════════
-- 工作流阶段（DAG 节点）
-- ═══════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS workflow_stages (
    id              TEXT PRIMARY KEY,           -- UUID
    workflow_id     TEXT NOT NULL,
    preset_id       TEXT,                       -- NULL 表示由编排器自行决定
    stage_order     INTEGER NOT NULL,           -- 顺序，相同值表示并行
    stage_group     TEXT DEFAULT 'default',     -- 并行分组标签
    input_template  TEXT NOT NULL DEFAULT '{{task}}',
        -- 模板变量: {{task}} {{previous.output}} {{stage.<stage_id>.output}}
    output_key      TEXT,                       -- 下游引用键名
    condition       TEXT NOT NULL DEFAULT 'always',
        -- always | on_success | on_failure | {{previous.status}} == 'success'
    timeout_secs    INTEGER NOT NULL DEFAULT 300,
    retry_count     INTEGER NOT NULL DEFAULT 0,
    auto_approve    INTEGER NOT NULL DEFAULT 0, -- 此阶段是否自动批准
    FOREIGN KEY (workflow_id) REFERENCES workflows(id) ON DELETE CASCADE,
    FOREIGN KEY (preset_id)   REFERENCES presets(id)   ON DELETE SET NULL
);

CREATE INDEX idx_stages_workflow ON workflow_stages(workflow_id, stage_order);

-- ═══════════════════════════════════════════════════════════════
-- 执行历史
-- ═══════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS workflow_runs (
    id              TEXT PRIMARY KEY,           -- UUID
    workflow_id     TEXT NOT NULL,
    workflow_name   TEXT NOT NULL,              -- 快照名称（workflow 可能被后续修改）
    trigger         TEXT NOT NULL DEFAULT 'manual',  -- manual|api|schedule|webhook
    status          TEXT NOT NULL DEFAULT 'pending', -- pending|running|success|failed|cancelled
    task            TEXT NOT NULL,              -- 用户原始输入
    started_at      TEXT,
    finished_at     TEXT,
    total_tokens    INTEGER DEFAULT 0,
    error_message   TEXT,
    FOREIGN KEY (workflow_id) REFERENCES workflows(id) ON DELETE SET NULL
);

CREATE INDEX idx_runs_workflow ON workflow_runs(workflow_id);
CREATE INDEX idx_runs_status ON workflow_runs(status);
CREATE INDEX idx_runs_started ON workflow_runs(started_at DESC);

-- ═══════════════════════════════════════════════════════════════
-- 阶段执行结果
-- ═══════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS stage_results (
    id              TEXT PRIMARY KEY,           -- UUID
    run_id          TEXT NOT NULL,
    stage_id        TEXT NOT NULL,
    stage_order     INTEGER NOT NULL,          -- 快照
    preset_name     TEXT,                      -- 快照预设名
    status          TEXT NOT NULL DEFAULT 'pending',  -- pending|running|success|failed|skipped
    input_prompt    TEXT,                      -- 模板渲染后的完整 prompt
    output_text     TEXT,                      -- 阶段输出
    output_summary  TEXT,                      -- 输出摘要 (前 500 字)
    tokens_used     INTEGER DEFAULT 0,
    tool_calls      TEXT DEFAULT '[]',         -- JSON 数组 ["read_file","write_file"]
    started_at      TEXT,
    finished_at     TEXT,
    error_message   TEXT,
    retry_attempt   INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (run_id)  REFERENCES workflow_runs(id)  ON DELETE CASCADE,
    FOREIGN KEY (stage_id) REFERENCES workflow_stages(id) ON DELETE SET NULL
);

CREATE INDEX idx_sr_run ON stage_results(run_id, stage_order);

-- ═══════════════════════════════════════════════════════════════
-- 用户偏好（跨项目）
-- ═══════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS user_preferences (
    key             TEXT PRIMARY KEY,           -- e.g. "default_model","language"
    value           TEXT NOT NULL,
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ═══════════════════════════════════════════════════════════════
-- 初始数据：用户偏好默认值
-- ═══════════════════════════════════════════════════════════════
INSERT OR IGNORE INTO user_preferences (key, value) VALUES
    ('language', 'zh'),
    ('theme', 'dark'),
    ('max_history', '50');
```

---

## 5. 模块设计

### 5.1 `src/db.rs` — 数据库层

```rust
//! 全局 SQLite 数据库（WAL 模式，多进程安全）
//!
//! # 使用示例
//! ```ignore
//! let db = GlobalDb::open_or_create()?;
//! let presets = db.list_presets()?;
//! db.save_preset(&Preset { name: "test".into(), ..Default::default() })?;
//! ```

use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;

/// 全局数据库句柄（每个进程一个实例）
pub struct GlobalDb {
    conn: Mutex<Connection>,
    path: PathBuf,
}

impl GlobalDb {
    /// 打开或创建全局数据库。
    /// 自动执行迁移和 pragma 设置。
    pub fn open_or_create() -> rusqlite::Result<Self> { ... }

    /// 返回数据库文件路径
    pub fn path(&self) -> &PathBuf { &self.path }

    // ── Preset CRUD ──────────────────────────────────────────
    pub fn list_presets(&self) -> rusqlite::Result<Vec<Preset>> { ... }
    pub fn get_preset(&self, id: &str) -> rusqlite::Result<Option<Preset>> { ... }
    pub fn save_preset(&self, preset: &Preset) -> rusqlite::Result<()> { ... }
    pub fn delete_preset(&self, id: &str) -> rusqlite::Result<()> { ... }

    // ── Workflow CRUD ────────────────────────────────────────
    pub fn list_workflows(&self) -> rusqlite::Result<Vec<Workflow>> { ... }
    pub fn get_workflow(&self, id: &str) -> rusqlite::Result<Option<Workflow>> { ... }
    pub fn save_workflow(&self, wf: &Workflow) -> rusqlite::Result<()> { ... }
    pub fn delete_workflow(&self, id: &str) -> rusqlite::Result<()> { ... }

    // ── Workflow Stages ──────────────────────────────────────
    pub fn save_stages(&self, workflow_id: &str, stages: &[Stage]) -> rusqlite::Result<()> { ... }

    // ── Execution History ────────────────────────────────────
    pub fn create_run(&self, run: &WorkflowRun) -> rusqlite::Result<()> { ... }
    pub fn update_run_status(&self, id: &str, status: &str) -> rusqlite::Result<()> { ... }
    pub fn save_stage_result(&self, sr: &StageResult) -> rusqlite::Result<()> { ... }
    pub fn list_runs(&self, limit: usize) -> rusqlite::Result<Vec<WorkflowRun>> { ... }

    // ── Preferences ──────────────────────────────────────────
    pub fn get_pref(&self, key: &str) -> rusqlite::Result<Option<String>> { ... }
    pub fn set_pref(&self, key: &str, value: &str) -> rusqlite::Result<()> { ... }
}

/// 数据模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub server_url: String,
    pub workdir: Option<String>,
    pub model: Option<String>,
    pub auto_approve: bool,
    pub agent_mode: String,     // auto|simple|plan|pipeline
    pub isolation: String,      // normal|container|sandbox
    pub new_session: bool,
    pub icon: String,
    pub color: Option<String>,
    pub tags: Vec<String>,
    pub sort_order: i32,
    pub created_at: String,
    pub updated_at: String,
}

// ... (Workflow, Stage, WorkflowRun, StageResult 类似)
```

### 5.2 `src/db/migration.rs` — 迁移系统

```rust
/// 迁移定义
const MIGRATIONS: &[(i32, &str, &str)] = &[
    (1, "initial_schema", include_str!("../../sql/migrations/001_initial.sql")),
    // (2, "add_tags_to_presets", include_str!("...")),
];

/// 自动迁移到最新版本
pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let current = get_version(conn)?;
    for &(version, name, sql) in MIGRATIONS {
        if version > current {
            conn.execute_batch(sql)?;
            set_version(conn, version, name)?;
            tracing::info!("DB migration {} ({}) applied", version, name);
        }
    }
    Ok(())
}
```

### 5.3 `src/orchestrator.rs` — 编排引擎

```rust
/// 工作流编排引擎
///
/// 从 global.db 加载工作流定义，按 DAG 顺序执行阶段，
/// 支持模板渲染、条件跳过、重试、并行执行。
pub struct Orchestrator {
    db: Arc<GlobalDb>,
    output: Arc<dyn AgentOutput>,
}

impl Orchestrator {
    pub fn new(db: Arc<GlobalDb>, output: Arc<dyn AgentOutput>) -> Self { ... }

    /// 按名称执行一个工作流
    ///
    /// 1. 从 DB 加载 workflow + stages
    /// 2. 创建 WorkflowRun 记录
    /// 3. 按 stage_order 分组，组内并行、组间串行
    /// 4. 对每个 stage 渲染 input_template（注入 {{task}} {{previous.output}} 等）
    /// 5. 调用对应 preset 的 agent 执行
    /// 6. 收集输出，记录到 stage_results
    /// 7. 返回汇总
    pub async fn execute(
        &self,
        workflow_id: &str,
        task: &str,
    ) -> anyhow::Result<WorkflowRunSummary> { ... }

    /// 列出所有可用工作流
    pub async fn list_workflows(&self) -> anyhow::Result<Vec<WorkflowInfo>> { ... }
}
```

### 5.4 新增 WebSocket API

Worker 通过现有 WebSocket 协议暴露新的消息类型：

```json
// ── Preset 管理 ──
{ "type": "preset_list",   "id": "req-1" }
{ "type": "preset_get",    "id": "req-2", "data": { "id": "xxx" } }
{ "type": "preset_save",   "id": "req-3", "data": { "preset": {...} } }
{ "type": "preset_delete", "id": "req-4", "data": { "id": "xxx" } }

// ── Workflow 管理 ──
{ "type": "workflow_list",   "id": "req-5" }
{ "type": "workflow_get",    "id": "req-6", "data": { "id": "xxx" } }
{ "type": "workflow_save",   "id": "req-7", "data": { "workflow": {...} } }
{ "type": "workflow_delete", "id": "req-8", "data": { "id": "xxx" } }

// ── 工作流执行 ──
{ "type": "workflow_execute", "id": "req-9", "data": {
    "workflow_id": "xxx",
    "task": "用户输入的任务描述"
}}

// ── 执行历史 ──
{ "type": "workflow_history", "id": "req-10", "data": { "limit": 20 } }
{ "type": "workflow_run_detail", "id": "req-11", "data": { "run_id": "xxx" } }

// ── 响应 ──
{ "type": "preset_list_result", "id": "req-1", "data": { "presets": [...] } }
{ "type": "preset_saved",       "id": "req-3", "data": { "id": "xxx" } }
{ "type": "workflow_progress",  "data": {
    "run_id": "xxx",
    "stage_order": 2,
    "status": "running",
    "message": "正在执行代码审查..."
}}
{ "type": "workflow_completed", "data": {
    "run_id": "xxx",
    "status": "success",
    "summary": "..."
}}
```

---

## 6. 模板引擎

### 6.1 Stage 输入模板

支持以下变量：

| 变量 | 含义 | 示例 |
|------|------|------|
| `{{task}}` | 用户原始输入 | "请帮我重构整个用户系统" |
| `{{previous.output}}` | 上一阶段（同 group 内前一个）的输出 | |
| `{{stage.<stage_id>.output}}` | 指定阶段的输出 | `{{stage.plan.outputs.plan}}` |
| `{{stage.<stage_id>.summary}}` | 指定阶段的摘要 | |
| `{{stages}}` | 所有已完成阶段的 JSON | |

### 6.2 示例

```toml
# workflow: 全流程代码审查
[[stages]]
preset = "planner"
input_template = """
你需要为以下任务制定详细计划：

任务：{{task}}

请输出：1) 步骤分解 2) 风险点 3) 涉及文件
"""
output_key = "plan"

[[stages]]
preset = "coder"
input_template = """
请按以下计划执行编码：

计划：
{{stage.plan.output}}

原始任务：{{task}}
"""
output_key = "code"

[[stages]]
preset = "reviewer"
input_template = """
请审查以下代码变更，检查：
- 逻辑正确性
- 性能问题
- 安全隐患

计划：{{stage.plan.summary}}
代码变更：{{stage.code.output}}
"""
condition = "on_success"  # 前两阶段都成功才执行
```

---

## 7. Web UI 变更

### 7.1 概览

```
┌──────────────────────────────────────────────┐
│  [Presets]  [Workflows]  [History]           │  ← 新标签页
├──────────────────────────────────────────────┤
│                                              │
│  ┌─────────────────────────────────────┐     │
│  │ ★ 全流程代码审查                     │     │
│  │   规划 → 编码 → 审查                 │     │
│  │   [▶ 执行]  [✎ 编辑]  [🗑 删除]      │     │
│  ├─────────────────────────────────────┤     │
│  │ ★ 前端后端并行开发                    │     │
│  │   分析 ─┬→ 前端                        │     │
│  │        └→ 后端 ─→ 集成                 │     │
│  │   [▶ 执行]  [✎ 编辑]  [🗑 删除]      │     │
│  └─────────────────────────────────────┘     │
│                                              │
│  [+ 新建工作流]                              │
└──────────────────────────────────────────────┘
```

### 7.2 Workflow 编辑器

- **可视化 DAG 编辑**（Phase 2）：拖拽 preset 卡片，连线建立依赖
- **模板输入**：每个阶段可编辑 `input_template`，实时预览变量
- **条件配置**：选择 `always / on_success / on_failure`
- **并行标记**：相同 `stage_order` 的阶段自动并行

### 7.3 执行进度

执行时在现有对话流中显示进度卡片：

```
┌─────────────────────────────────────────┐
│ 🔄 工作流: 全流程代码审查                  │
│                                         │
│ ✅ 1. 规划 (deepseek_pro)    [1.2s]     │
│ ⚙️  2. 编码 (deepseek_flash)  [运行中...] │
│ ⏳ 3. 审查 (qwan35)                      │
└─────────────────────────────────────────┘
```

---

## 8. 实施计划

### Phase 1: 数据库基础设施（本次）

| 文件 | 内容 |
|------|------|
| `Cargo.toml` | 添加 `rusqlite = { version = "0.31", features = ["bundled"] }` |
| `src/db.rs` | GlobalDb 结构体 + 连接管理 + pragma 设置 |
| `src/db/migration.rs` | 迁移框架 + v1 初始 migration |
| `sql/migrations/001_initial.sql` | 完整 DDL |
| `src/db/models.rs` | Preset / Workflow / Stage / WorkflowRun / StageResult 数据模型 |

### Phase 2: Preset 迁移 + CRUD API

| 文件 | 内容 |
|------|------|
| `src/db.rs` | Preset CRUD 方法 |
| `src/worker.rs` | 处理 `preset_*` WebSocket 消息 |
| `web-ui/src/stores/agentStore.ts` | 从 localStorage 迁移到服务端 API |

### Phase 3: 工作流定义 CRUD

| 文件 | 内容 |
|------|------|
| `src/db.rs` | Workflow + Stage CRUD 方法 |
| `src/worker.rs` | 处理 `workflow_*` WebSocket 消息 |
| `web-ui/src/components/` | Workflow 列表 + 编辑器面板 |

### Phase 4: 编排引擎

| 文件 | 内容 |
|------|------|
| `src/orchestrator.rs` | 工作流引擎（模板渲染、阶段调度、call_node 集成） |
| `src/worker.rs` | 处理 `workflow_execute` 消息 |
| `web-ui/` | 执行进度 UI |

### Phase 5: 执行历史 + 高级特性

| 文件 | 内容 |
|------|------|
| `src/db.rs` | 查询历史 + 统计分析 |
| `web-ui/` | 执行历史面板、重跑、条件分支可视化 |
| `src/orchestrator.rs` | 并行阶段支持、失败重试、超时处理 |

---

## 9. 风险 & 缓解

| 风险 | 缓解措施 |
|------|----------|
| SQLITE_BUSY 多进程冲突 | WAL + busy_timeout + 应用层重试 |
| DB 文件损坏 | WAL 模式 crash-safe；定期备份即可 |
| 迁移失败导致 DB 不可用 | 每个 migration 在事务内执行；失败回滚 |
| localStorage → DB 迁移数据丢失 | 迁移前自动备份 localStorage 到 JSON 文件 |
| rusqlite bundled 编译慢 | 首次编译多 ~30s，后续增量编译无影响 |

---

## 10. 未来扩展

| 特性 | 依赖 | 说明 |
|------|------|------|
| 定时任务 | Phase 3+ | `workflow_runs.trigger = "schedule"`，cron 表达式 |
| Webhook 触发 | Phase 3+ | `trigger = "webhook"`，HTTP endpoint |
| 跨设备同步 | 全局 DB | 将 `global.db` 放入 syncthing/Dropbox 同步目录 |
| 条件分支 | Phase 5 | `condition` 字段支持 `{{stage.x.status}} == 'success'` |
| 子工作流调用 | Phase 5 | Stage 的 preset 可为 `null`，引用另一个 workflow |

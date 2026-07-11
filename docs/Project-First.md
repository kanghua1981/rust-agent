# 🏗️ Web-UI 深度重构计划：Project-First 架构（防断裂版）

## 核心策略：加法先行，减法殿后

原始计划的问题是 **每个 Phase 都在「就地重命名」**——改了 store 字段名但组件还引旧名字，立刻全炸。

修正后的策略只有一条铁律：

> **永远「先加新的、保持旧的、验证通过、再删旧的」**。任何时候 `tsc --noEmit` 必须零错误通过。

---

## 目标架构

```
Projects (localStorage 持久化)
├── Project "my-frontend"
│   ├── 定义: { serverUrl, workdir, isolation, label }
│   ├── 会话列表:
│   │   ├── "feature-login" (15条消息)
│   │   ├── "bugfix-123"  (42条消息)  ← active
│   └── WebSocket 连接状态: connected / idle
├── Project "my-backend"
│   ├── 定义: { serverUrl, workdir, ... }
│   └── 会话: "default" (3条)
└── Project "adhoc-task"
    └── 会话: "default" (0条, 未连接)
```

---

## Phase 0 — 准备：迁移工具 + 验证机制（先做，不等最后）

> ⚠️ **这是最关键的结构性修正**：旧计划把 migration 放在 Phase 9（最后），但 Zustand persist 在 store 初始化时就读 localStorage。一旦字段改名，旧数据就丢了。必须在改字段名前准备好迁移。

### Step 0.1: 新建 `migration.ts`

**文件:** `web-ui/src/utils/migration.ts`（新建）

```typescript
// 一次性迁移：旧 localStorage 数据 → 新 ProjectDefinition[] 格式
// 幂等：设置标记 _migrated_v2，重复执行无副作用

const MIGRATION_KEY = '_migrated_project_first_v1';

export interface MigrationResult {
  projects: ProjectDefinition[];
  migrated: boolean;
}

export function runMigration(): MigrationResult {
  if (localStorage.getItem(MIGRATION_KEY)) {
    return { projects: [], migrated: false };
  }
  try {
    const raw = localStorage.getItem('rust-agent-config');
    if (!raw) return { projects: [], migrated: false };
    const parsed = JSON.parse(raw);
    const data = parsed.state ?? parsed;
    const projects: ProjectDefinition[] = [];
    const seen = new Set<string>();

    // 1. presets → projects
    for (const p of (data.presets || [])) {
      if (!p.id || seen.has(p.id)) continue;
      projects.push({
        id: p.id,
        label: p.label || p.workdir?.split('/').filter(Boolean).pop() || p.serverUrl || '',
        serverUrl: p.serverUrl || '',
        workdir: p.workdir || '',
        isolation: p.isolation || 'normal',
        agentMode: p.agentMode || 'auto',
        autoApprove: p.autoApprove ?? false,
        newSessionOnConnect: p.newSession ?? false,
        createdAt: p.createdAt || new Date().toISOString(),
        updatedAt: p.updatedAt || new Date().toISOString(),
      });
      seen.add(p.id);
    }
    // 2. connectionHistory → projects
    for (const h of (data.connectionHistory || [])) {
      const id = h.projectId || h.id;
      if (seen.has(id)) continue;
      projects.push({
        id,
        label: h.workdir?.split('/').filter(Boolean).pop() || h.serverUrl || '',
        serverUrl: h.serverUrl || '',
        workdir: h.workdir || '',
        isolation: 'normal',
        agentMode: 'auto',
        autoApprove: false,
        newSessionOnConnect: false,
        createdAt: new Date(h.connectedAt || Date.now()).toISOString(),
        updatedAt: new Date(h.lastConnectedAt || Date.now()).toISOString(),
      });
      seen.add(id);
    }
    // 不删旧 key，只写标记
    localStorage.setItem(MIGRATION_KEY, '1');
    return { projects, migrated: projects.length > 0 };
  } catch (e) {
    console.warn('Migration failed:', e);
    return { projects: [], migrated: false };
  }
}
```

### Step 0.2: store 初始化时集成迁移

**文件:** `web-ui/src/stores/agentStore.ts`

在 `loadPersistedConfig()` 函数中，Zustand persist 读取旧数据之前，调用 `runMigration()`，将迁移出的 `projects` 合并到初始状态。

**验证:** 打开浏览器 → console 确认迁移标记已设 → 关闭 → 再打开 → 确认不再重复迁移

### Step 0.3: 添加编译验证脚本

**文件:** `web-ui/package.json`（已有 `tsc` 的话只需确认）

```bash
# 每次改完一个 Step 立即跑:
cd web-ui && npx tsc --noEmit
```

---

## Phase 1 — 类型层：确认完成度（大部分已就绪）

当前状态:
- ✅ `ProjectDefinition` 接口已存在 (agent.ts L1080)
- ✅ `ProjectSlot` 接口已存在 (agent.ts L1110)
- ✅ `ConnectionSlot = ProjectSlot` 别名已存在 (agent.ts L1107)
- ✅ `ConnectionHistory.projectId?` 已加 (agent.ts L1098)
- ❌ `createEmptySlot()` 没设 `projectId` 字段

### Step 1.1: 补全 `createEmptySlot` 的 projectId

**文件:** `web-ui/src/stores/agentStore.ts` L30

```typescript
// 改前:
function createEmptySlot(id: string, label: string, serverUrl: string, workdir?: string): ConnectionSlot {
  return { id, label, serverUrl, workdir, ... };

// 改后: 增加 projectId 参数，默认 = id
function createEmptySlot(id: string, label: string, serverUrl: string, workdir?: string, projectId?: string): ConnectionSlot {
  return { id, projectId: projectId || id, label, serverUrl, workdir, ... };
}
```

**验证:** `npx tsc --noEmit` 零错误

---

## Phase 2 — 状态管理层：并行字段，不同时删除旧名

> 🔑 **这是整个计划的核心修正**。旧计划在 Step 2.1 一次性把 `connections` → `projectSlots`，`activeConnectionId` → `activeProjectId`，导致 store 内部 100+ 处引用瞬间断裂。
>
> 新策略：每个旧字段旁边 **加一个同义的、同步维护的新字段**。组件可以逐个迁移到新字段，旧字段在整个 Phase 3 期间保持不变。

### Step 2.1: 新增 `projects` 字段 + CRUD（独立于 presets）

**文件:** `web-ui/src/stores/agentStore.ts`

```typescript
// AgentState 接口中新增（presets 保留不动）:
projects: Record<string, ProjectDefinition>;

// 新增 actions（与 presets 的 addPreset/updatePreset/deletePreset 并存）:
addProject: (p: ProjectDefinition) => void;
updateProject: (id: string, updates: Partial<ProjectDefinition>) => void;
deleteProject: (id: string) => void;
```

实现: `addProject` / `updateProject` / `deleteProject` 操作 `projects` 字段，并通过 Zustand persist 写入 localStorage。

**验证:** 在浏览器 console 执行 `useAgentStore.getState().addProject({...})` → 读回 `projects` → 刷新页面 → 确认数据持久化

### Step 2.2: 新增 `projectSlots` 字段（与 `connections` 同时写入）

**文件:** `web-ui/src/stores/agentStore.ts`

这是最关键的一步——不是重命名 `connections`，而是在每个修改 `connections` 的地方 **同时写入 `projectSlots`**。

```typescript
// AgentState 接口:
projectSlots: Record<string, ProjectSlot>;   // 新增，与 connections 同内容

// initialState:
projectSlots: { [defaultId]: initialSlot },

// 修改点 1: createConnectionSlot — 结尾增加:
set({
  connections: updated,
  projectSlots: updated,   // ← 新增：同步写入
});

// 修改点 2: removeConnectionSlot — 结尾增加 projectSlots: rest
// 修改点 3: setActiveConnection — 结尾增加 projectSlots: updated
// 修改点 4: _updateSlot — 修改 connections 的同时改 projectSlots
// 修改点 5: _saveActiveSlot — 同上
// 修改点 6: 所有 syncActiveSlot/syncHeavyField — 修改 connections 的同时改 projectSlots
```

具体做法：修改 `syncActiveSlot` 内部逻辑，让它 **同时更新 `connections` 和 `projectSlots`**：

```typescript
const syncActiveSlot = (flatUpdates: Partial<AgentState>) => {
  const state = get() as any;
  const id = state.activeConnectionId;
  if (!id) return flatUpdates;
  const slotUpdates: any = {};
  for (const key of Object.keys(flatUpdates)) {
    if (state.connections?.[id] && key in state.connections[id]) {
      slotUpdates[key] = (flatUpdates as any)[key];
    }
  }
  if (Object.keys(slotUpdates).length === 0) return flatUpdates;
  const newConnections = { ...state.connections, [id]: { ...state.connections[id], ...slotUpdates } };
  return {
    ...flatUpdates,
    connections: newConnections,
    projectSlots: newConnections,   // ← 同步写入
  };
};
```

**验证:** `npx tsc --noEmit` → 在浏览器 console 中确认 `useAgentStore.getState().projectSlots` 和 `connections` 内容始终一致

### Step 2.3: 新增 `activeProjectId` 字段（与 `activeConnectionId` 同时维护）

```typescript
// AgentState 接口:
activeProjectId: string | null;

// initialState:
activeProjectId: defaultId,

// 修改点: 所有 set({ activeConnectionId: xxx }) 的地方，同时设置 activeProjectId
// 主要在:
//   - createConnectionSlot (不直接改 activeConnectionId，但...)
//   - removeConnectionSlot
//   - setActiveConnection
//   - reset
```

在 `setActiveConnection` 函数结尾的 `set()` 调用中增加 `activeProjectId: id`。

**验证:** `npx tsc --noEmit` → console 中 `useAgentStore.getState().activeProjectId` 始终等于 `activeConnectionId`

### Step 2.4: 新增 Project 语义的 actions（作为旧函数的包装）

```typescript
// 这些是新「门面」，内部调用旧函数，给组件迁移用:

openProject: (projectId: string) => {
  // 1. 从 projects[projectId] 读取 serverUrl/workdir
  // 2. 调用 createConnectionSlot(projectId, label, serverUrl, workdir, projectId)
  // 3. 调用 setActiveConnection(projectId)
  // 4. 不负责 WebSocket 连接（由 useWebSocket 的 connect(slotId) 处理）
},

closeProject: (projectId: string) => {
  // 调用 removeConnectionSlot(projectId)
  // useWebSocket 侧需同步断开
},

setActiveProject: (projectId: string) => {
  // 调用 setActiveConnection(projectId)
},
```

**验证:** console 中调用 `openProject('test-id')` → 确认 `connections` 和 `projectSlots` 中都出现了新 slot

### Step 2.5: 验证 checkpoint

此时状态：
- ✅ `projects` / `projectSlots` / `activeProjectId` 与 `presets` / `connections` / `activeConnectionId` 并存
- ✅ 旧组件（App.tsx / ConnectionTabs / Sidebar 等）仍通过旧名字工作，完全不受影响
- ✅ `npx tsc --noEmit` 零错误
- ✅ 浏览器中所有现有功能正常

**这是整个重构的第一个「安全据点」**——从这里开始，每个后续 Step 都可以独立验证。

---

## Phase 3 — 组件层渐进迁移

> 每个 Step 只迁移 **一个组件** 到新名字。改完立即 `tsc --noEmit` + 手动测试。一个组件坏了不影响其他。

### Step 3.1: 新建 `ProjectTree` 组件

**文件:** `web-ui/src/components/ProjectTree.tsx`（新建）

纯展示组件，从 store 读 `projects` 和 `projectSlots`：

```tsx
// 读取:
const projects = useAgentStore(s => s.projects);
const projectSlots = useAgentStore(s => s.projectSlots);
const activeProjectId = useAgentStore(s => s.activeProjectId);
const openProject = useAgentStore(s => s.openProject);
const closeProject = useAgentStore(s => s.closeProject);
const setActiveProject = useAgentStore(s => s.setActiveProject);
```

渲染项目树（参考原文档 Phase 3.1 的 UI 设计）。

**验证:** 组件独立可用 → 暂时不集成到 Sidebar → `npx tsc --noEmit`

### Step 3.2: 集成 `ProjectTree` 到 `Sidebar`

**文件:** `web-ui/src/components/Sidebar.tsx`

- 在侧边栏顶部插入 `<ProjectTree />`
- **暂时保留** `PresetsSection`（旁侧并存）
- 连接状态从 `projectSlots[activeProjectId].connectionStatus` 读取

**验证:** 侧边栏同时显示「项目树」和「旧预设列表」→ 两者都可用

### Step 3.3: 新建 `ProjectTabs` 组件（替代 `ConnectionTabs`）

**文件:** `web-ui/src/components/ProjectTabs.tsx`（新建）

- 从 `projectSlots` / `activeProjectId` 读取
- 调用 `setActiveProject` 切换
- 调用 `closeProject` 关闭
- `ConnectionTabs.tsx` 暂时保留不动

**验证:** 两个组件并存 → ProjectTabs 功能正确 → `npx tsc --noEmit`

### Step 3.4: `App.tsx` 切换到 `ProjectTabs`

**文件:** `web-ui/src/App.tsx`

```typescript
// 改前:
import { ConnectionTabs } from './components/ConnectionTabs';
<ConnectionTabs onNewConnection={...} switchToConnection={...} disconnectSlot={...} connectSlot={...} />

// 改后:
import { ProjectTabs } from './components/ProjectTabs';
<ProjectTabs onNewProject={...} />
```

同时将 `handleConnect` 改为调用 `openProject` + `connect`。

**验证:** Tab 切换正常工作 → `npx tsc --noEmit`

### Step 3.5: 新建 `ProjectDialog` 组件（替代 `ConnectModal`）

**文件:** `web-ui/src/components/ProjectDialog.tsx`（新建）

- 读/写 `projects`（通过 `addProject` / `updateProject`）
- 两种模式：添加项目 / 编辑项目
- UI 参考原文档 Phase 5.1

**验证:** 新建项目 → 确认存入 `projects` → 刷新后仍在

### Step 3.6: `App.tsx` 切换到 `ProjectDialog`

```typescript
// 改前:
import { ConnectModal } from './components/ConnectModal';
{showConnect && <ConnectModal ... />}

// 改后:
import { ProjectDialog } from './components/ProjectDialog';
{showConnect && <ProjectDialog ... />}
```

### Step 3.7: `SessionsPanel` 切换到新字段

**文件:** `web-ui/src/components/SessionsPanel.tsx`

- 从 `activeProjectId` + `projectSlots` 读取当前项目
- 会话列表绑定到当前 project 的 workdir

### Step 3.8: `Header` 切换到新字段

**文件:** `web-ui/src/components/Header.tsx`

- 显示 `projects[activeProjectId]?.label` 替代 serverUrl
- 会话切换下拉菜单

### Step 3.9: `WorkflowPanel` presets 引用切换

**文件:** `web-ui/src/components/WorkflowPanel.tsx`

- 将 `presets.find(...)` 改为 `projects` 查找（或同时查两者）

**验证:** `npx tsc --noEmit` → 工作流仍可引用项目

---

## Phase 4 — 清理：删除旧名字

> 此处所有旧名字已无消费者（tsc 会验证），可以安全删除。

### Step 4.1: 删除 `ConnectionSlot` 类型别名

**文件:** `web-ui/src/types/agent.ts`

```typescript
// 删除这行:
// export type ConnectionSlot = ProjectSlot;
```

如果还有引用，`tsc --noEmit` 会报错，逐个改成 `ProjectSlot`。

### Step 4.2: 删除 store 旧字段

**文件:** `web-ui/src/stores/agentStore.ts`

删除以下字段和相关代码：
- `connections`（已被 `projectSlots` 替代）
- `activeConnectionId`（已被 `activeProjectId` 替代）
- `createConnectionSlot` / `removeConnectionSlot`（已被 `openProject` / `closeProject` 替代）
- `setActiveConnection`（已被 `setActiveProject` 替代）
- `presets` 及其 CRUD（已被 `projects` 替代）
- `connectionHistory` 及其 actions

`syncActiveSlot` / `syncHeavyField` 内部改为只操作 `projectSlots[activeProjectId]`。

### Step 4.3: 删除废弃组件

- 删除 `ConnectionTabs.tsx`
- 删除 `ConnectModal.tsx`
- 删除 `Sidebar.tsx` 中的 `PresetsSection`

### Step 4.4: 删除废弃类型

- 删除 `ConfigPreset` 类型（`types/agent.ts`）
- 删除 `ConnectionHistory` 类型（如已无引用）

### Step 4.5: useWebSocket 清理

**文件:** `web-ui/src/hooks/useWebSocket.ts`

- 将 `getActiveSlotId()` 改为读 `activeProjectId`
- 删除 `createConnectionSlot` / `removeConnectionSlot` 的解构引用
- 删除 `listPresets` / `savePreset` / `deletePreset` 相关代码

### Step 4.6: 端到端验证

1. 清除 localStorage → 打开页面 → 新建项目 → 连接 → 发送消息 → ✅
2. 切换会话 → 切换项目 Tab → 验证消息隔离 → ✅
3. 关闭 Tab → 重新连接 → 验证会话恢复 → ✅
4. 旧数据迁移 → 检查旧 presets 正确变成 projects → ✅
5. `npx tsc --noEmit` 零错误 → ✅

---

## Phase 5 — 后端协议增强（可选）

### Step 5.1: Worker 支持 `list_projects` 消息

**文件:** `src/worker.rs`

Worker 收到 `list_projects` 时扫描 `.agent/sessions/` 目录，返回会话摘要。

### Step 5.2: URL 参数 `?project=` 支持

**文件:** `src/server.rs`

`ws://host/agent?project=<id>` → 自动解析 workdir + session。

---

## 执行顺序依赖图（修正版）

```
Phase 0 (迁移工具+验证)        ← 最优先！
    ↓
Phase 1 (类型层补全)           ← 很小，30分钟
    ↓
Phase 2.1 (projects 字段)     ← 新增，不影响旧功能
    ↓
Phase 2.2 (projectSlots 并行) ← 新增，不影响旧功能
    ↓
Phase 2.3 (activeProjectId)   ← 新增，不影响旧功能
    ↓
Phase 2.4 (新 actions 包装)   ← 新增，不影响旧功能
    ↓
Phase 2.5 (CHECKPOINT ✅)     ← 此时新旧两套并存，所有功能正常
    ↓
Phase 3.1 (ProjectTree)       ← 新组件，独立可测
    ↓
Phase 3.2 (Sidebar 集成)      ←      ↓
    ↓                              ↓
Phase 3.3 (ProjectTabs)       ← Phase 3.5 (ProjectDialog)
    ↓                              ↓
Phase 3.4 (App 切换 Tabs)     ← Phase 3.6 (App 切换 Dialog)
    ↓
Phase 3.7 (SessionsPanel)
    ↓
Phase 3.8 (Header)
    ↓
Phase 3.9 (WorkflowPanel)
    ↓
Phase 4.1-4.5 (清理旧名字)    ← 此时旧名字已无消费者
    ↓
Phase 4.6 (E2E 验证)
    ↓
Phase 5 (后端协议 - 可选)
```

---

## 修正后工作量

| Phase | 内容 | 行数 | 耗时 |
|---|---|---|---|
| Phase 0 | migration.ts + 集成 | ~100 | 30 min |
| Phase 1 | createEmptySlot 补字段 | ~5 | 10 min |
| Phase 2.1 | projects 字段 + CRUD | ~80 | 30 min |
| Phase 2.2 | projectSlots 并行写入 | ~40 | 45 min |
| Phase 2.3 | activeProjectId 并行 | ~20 | 20 min |
| Phase 2.4 | 新 actions 包装 | ~50 | 30 min |
| Phase 2.5 | checkpoint 验证 | - | 15 min |
| Phase 3.1 | ProjectTree 组件 | ~250 | 2 hours |
| Phase 3.2 | Sidebar 集成 | ~30 | 20 min |
| Phase 3.3 | ProjectTabs 组件 | ~200 | 1 hour |
| Phase 3.4 | App 切换 Tabs | ~30 | 15 min |
| Phase 3.5 | ProjectDialog 组件 | ~400 | 2 hours |
| Phase 3.6 | App 切换 Dialog | ~20 | 10 min |
| Phase 3.7 | SessionsPanel | ~50 | 30 min |
| Phase 3.8 | Header | ~60 | 30 min |
| Phase 3.9 | WorkflowPanel | ~20 | 15 min |
| Phase 4 | 清理删除 | ~300 | 1 hour |
| Phase 5 | 后端 | ~50 | 30 min |
| **总计** | | **~1700 行** | **约 11 小时** |

---

## 风险对照表

| 原始风险 | 旧缓解措施 | 修正后 |
|---|---|---|
| store 字段重命名 → 大面积断裂 | 「TypeScript 编译时捕获」❌ | 不重命名——加新字段并行，旧字段保持到 Phase 4 才删 ✅ |
| 旧 localStorage 丢失 | 「Phase 9.2 迁移脚本」❌ | 迁移脚本在 Phase 0 最先执行 ✅ |
| 中途中断 → 整个前端不可用 | 无 ❌ | 每个 Step 独立可验证，中断在任何 Step 都能回退到上一个 ✅ |
| 影响面预估偏低 | 1800 行 ❌ (实际 3000+) | 1700 行 + 每个 Step 精准计行 ✅ |

# Isolation & Sandbox Architecture

## 概述

每个 WebSocket 连接对应一个独立的 **worker 子进程**。子进程的隔离程度由
**Isolation Mode（隔离模式）** 决定，分三档：

| 模式 | 值 | rootfs/namespace | overlayfs | /rollback | 工具兼容性 |
|------|----|-----------------|-----------|-----------|-----------|
| **Normal**    | `normal`    | ✗ | ✗ | ✗ | ✅ 完全 |
| **Container** | `container` | ✅ | ✗ | ✗ | ✅ 取决于 binds 配置 |
| **Sandbox**   | `sandbox`   | ✅ | ✅ | ✅ | ✅ 取决于 binds 配置 |

---

## 模式详解

### Normal（无容器）

```
server → spawn worker (直接 exec 当前二进制，传真实 project_dir)
worker 进程在宿主环境直接运行，无任何 namespace 或 rootfs 变换。
```

- 工具可访问宿主全部 PATH、设备、socket 等
- 无进程隔离，无文件系统隔离
- 适用：本地开发、完全信任的自动化任务

### Container（namespace + rootfs）

```
server → fork → pre_exec: setup_rootfs(overlay=false) → exec /agent --mode worker
worker 进程在新的 user+mount namespace 中运行，rootfs 由 tmpfs + bind-mount 构建。
/workspace → rw bind → 真实 project_dir （直写）
```

- 进程视图与宿主隔离（看不到 /home 等宿主目录）
- 写操作直接落到真实项目文件
- 适用：隔离执行任务（数据处理、代码生成）但需要直写结果

### Sandbox（namespace + rootfs + overlayfs）

```
server → fork → pre_exec: setup_rootfs(overlay=true) → exec /agent --mode worker
worker 进程在容器内运行，/workspace 是 overlayfs 合并视图。

  lower  = /workspace-ro  (ro-bind 真实项目，内核保证不可绕过)
  upper  = /tmp/overlay/upper  (tmpfs，所有写入落这里)
  work   = /tmp/overlay/work
  merged = /workspace  (tools 操作的路径)
```

- 进程视图与宿主隔离
- 所有写操作落到 tmpfs upper 层，原始文件一字不改
- 支持 `/rollback`（清空 upper）、`/commit`（将 upper apply 回真实目录）
- 适用：需要 commit/rollback 保护的编码任务

---

## 整体架构

```
server 进程（极简 TCP acceptor）
│
│  URL: ws://host:port/agent?workdir=...&mode=normal|container|sandbox
│
├── Normal:    spawn(real_exe, -d real_project_dir)        ← 无 pre_exec
├── Container: fork → pre_exec: setup_rootfs(overlay=false) → exec /agent
│                     -d /workspace
└── Sandbox:   fork → pre_exec: setup_rootfs(overlay=true)  → exec /agent
                      -d /workspace
                      └── overlayfs: lower=/workspace-ro upper=/tmp/overlay/upper

worker 进程
│
├── Normal    → project_dir = 真实路径，Sandbox::disabled()
├── Container → project_dir = /workspace (rw bind)，Sandbox::disabled()
└── Sandbox   → project_dir = /workspace (overlay merged)，Sandbox::from_overlay_dirs()
│
└── 连接断开 → 进程退出 → mount namespace 随之销毁，内核自动清理所有挂载
```

---

## setup_rootfs 执行流程

仅 Container / Sandbox 模式执行，在 `Command::pre_exec()` 钩子中（fork 后 exec 前，单线程）：

```
1. unshare(CLONE_NEWUSER | CLONE_NEWNS)
2. 写 uid_map / gid_map → namespace 内 uid=0（宿主侧仍是普通用户，无法提权）
3. mount --make-rprivate /        → 防止 mount 事件传播到宿主
4. 建立 /tmp/.agent-nr-{pid}（tmpfs）作为 newroot
5. 按 Mount 表 bind-mount 各路径到 newroot/
6. pivot_root(newroot, newroot/.old) + umount2(.old, MNT_DETACH)
7. mount tmpfs /tmp
8. [Sandbox 模式] mount overlayfs:
     lower=/workspace-ro upper=/tmp/overlay/upper work=/tmp/overlay/work → /workspace
```

---

## Mount 表

### 系统路径（硬编码，按发行版按需跳过）

| 宿主路径 | 容器内路径 | 类型 | 说明 |
|---------|-----------|------|------|
| `/usr` | `/usr` | ro-bind | 工具链、Python/Node/gcc 等 |
| `/lib` `/lib64` `/lib32` `/libx32` | 同名 | ro-bind | glibc 等基础库 |
| `/bin` `/sbin` | 同名 | ro-bind | 基础命令（通常是 /usr 的 symlink）|
| `/etc/hosts` | `/etc/hosts` | 文件复制 | hostname 解析 |
| `/etc/resolv.conf` | `/etc/resolv.conf` | 文件复制 | DNS（API 调用需要，避免 symlink 跨 ns 问题）|
| `/etc/ssl` `/etc/pki` `/etc/ca-certificates` | 同名 | ro-bind | HTTPS 证书验证 |
| `/proc` `/sys` `/dev` | 同名 | rw-bind（来自宿主）| 设备与进程信息 |
| `<exe>` | `/agent` | rw-bind | agent 二进制本身（exec 入口）|
| Container: `<project_dir>` | `/workspace` | rw-bind | 工作目录，直写 |
| Sandbox: `<project_dir>` | `/workspace-ro` | rw-bind* | 只读 lower 层 |
| Sandbox: （空目录） | `/workspace` | overlayfs merged | tools 实际操作路径 |

> \*理想情况是 ro-bind，但在非特权 user namespace 中 `MS_REMOUNT|MS_RDONLY`
> 被内核拒绝（MNT_LOCKED）。overlayfs 在内核层面保证 lower 层不可写，
> 无需 bind mount 的只读标志。

### 明确不挂载

| 路径 | 原因 |
|------|------|
| `/home` `/root` | 用户私有数据 |
| `/var` `/run` | 运行时状态，agent 不需要 |
| `/media` `/mnt` `/opt` | 按需通过 extra_binds 配置 |
| `~/.config` `~/.local` | 配置已通过 `--config-json` 传入 |

### 额外 bind（models.toml 配置，仅 Container/Sandbox 模式）

```toml
[[extra_binds]]
host = "/data/shared-knowledge"
target = "/knowledge"
readonly = true
```

---

## overlayfs 生命周期（Sandbox 模式）

```
worker 启动
  └─ setup_rootfs 在 pre_exec 挂载 overlayfs
       lower=/workspace-ro  upper=/tmp/overlay/upper  → merged=/workspace

运行中
  └─ tools 在 /workspace 读写（写入落到 /tmp/overlay/upper，tmpfs）

/changes
  └─ 遍历 upper 层，展示变更文件列表和 diff

/rollback
  └─ 清空 /tmp/overlay/upper，remount overlayfs（丢弃所有变更）
  └─ 原始 project_dir 完全未受影响

/commit
  └─ 遍历 upper，将变更 apply 到宿主 project_dir
  └─ 清空 upper，remount（干净状态继续工作）

连接断开
  └─ worker 进程退出 → mount namespace 销毁
  └─ 内核自动清理 overlayfs + tmpfs
  └─ /tmp/.agent-nr-{pid} 目录残留由下次 server 启动时清理
```

---

## URL 参数与命令行参数

```
# WebSocket 连接（客户端指定模式）
ws://host:port/agent?workdir=/path&mode=normal
ws://host:port/agent?workdir=/path&mode=container   ← 默认
ws://host:port/agent?workdir=/path&mode=sandbox

# 向后兼容
ws://host:port/agent?sandbox=1   → mode=sandbox
ws://host:port/agent?sandbox=0   → mode=container

# 命令行启动（设置服务器默认）
agent --mode server --isolation normal
agent --mode server --isolation container   ← 默认
agent --mode server --isolation sandbox

# CLI / TUI 本地使用
agent --isolation sandbox   → 启用 overlayfs 保护
agent --isolation normal    → 无隔离直接运行
```

---

## 安全边界

| 边界 | Normal | Container | Sandbox |
|------|--------|-----------|---------|
| 进程看到宿主目录 | ✅ 全部可见 | ✗ 仅 bind 进来的路径 | ✗ 仅 bind 进来的路径 |
| 写操作影响真实文件 | ✅ 直接影响 | ✅ 直接影响 | overlay 保护，需 /commit |
| 可提权到宿主 root | ✗（无法提权）| ✗（uid_map 映射）| ✗（uid_map 映射）|
| 网络隔离 | ✗（不隔离）| ✗（不隔离）| ✗（不隔离）|

> 网络不隔离是有意为之：agent 需要访问 Anthropic/OpenAI API。

---

## 实现文件

| 文件 | 职责 |
|------|------|
| `src/container.rs` | `IsolationMode` 枚举 + `setup_rootfs()` — `pre_exec` 里执行的纯 libc 代码 |
| `src/server.rs` | 解析 `?mode=` URL 参数，三路 spawn 逻辑 |
| `src/worker.rs` | `IsolationMode` 接收，Sandbox 句柄初始化 |
| `src/sandbox.rs` | `Sandbox` 结构体：overlay 挂载、changes、rollback、commit |
| `src/cli.rs` / `src/tui_app.rs` | CLI/TUI 模式下的 isolation 处理 |
| `src/config.rs` | `extra_binds: Vec<ExtraBindMount>` |

# Sandbox Architecture

## 目标

每个 WebSocket 连接对应一个独立的 worker 子进程，运行在隔离的虚拟 rootfs 中。
project_dir 以**只读 bind** 方式挂入容器，agent 的所有写操作通过 fuse-overlayfs
落在 tmpfs 的 upper 层，宿主文件一字不改。用户执行 commit 时才将 upper 层合并回
宿主 project_dir。

---

## 整体架构

```
server 进程（极简 TCP acceptor）
│
├── accept(fd=7) → pre_exec: setup_rootfs(project_dir, extra_binds)
│                  spawn worker --worker-fd=7 -d /workspace
└── 继续等待连接

  setup_rootfs() 在 pre_exec 钩子里执行（fork 后 exec 前，单线程）:
    1. unshare(CLONE_NEWUSER | CLONE_NEWNS)
    2. write uid_map / gid_map  → namespace 内 uid=0（宿主侧仍是普通用户）
    3. 建立 /newroot（tmpfs）
    4. 按 mount 表挂载各路径
    5. pivot_root("/newroot", "/newroot/.old") + umount2(".old", MNT_DETACH)
    → exec agent --mode worker ...

worker 进程（在虚拟 rootfs 里运行）
│
├── /workspace     ← 只读（project_dir 的 ro-bind）
├── /workspace-rw  ← fuse-overlayfs 合并视图（agent 实际操作这里）
│     lower = /workspace
│     upper = /tmp/upper  (tmpfs)
│     work  = /tmp/work   (tmpfs)
│
├── agent.project_dir = /workspace-rw
│
├── commit  → 将 /tmp/upper/ 的变更 apply 回宿主 project_dir
└── rollback → 清空 /tmp/upper/，重新挂载 overlay

连接断开 → 进程退出 → mount namespace 随之销毁，内核自动清理所有挂载
```

---

## Mount 表（最终决策）

### 系统路径（在 pre_exec setup_rootfs 中硬编码）

| 宿主路径 | 容器内路径 | 类型 | 说明 |
|---------|-----------|------|------|
| `/usr` | `/usr` | ro-bind | 工具链、Python/Node/gcc 等 |
| `/lib` | `/lib` | ro-bind | glibc 等基础库 |
| `/lib64` | `/lib64` | ro-bind | 64位动态链接器 |
| `/bin` | `/bin` | ro-bind | 基础命令（bash/ls等），通常是 symlink |
| `/sbin` | `/sbin` | ro-bind | fusermount3 等系统工具 |
| `/etc/resolv.conf` | `/etc/resolv.conf` | ro-bind | DNS（API 调用需要） |
| `/etc/ssl/certs` | `/etc/ssl/certs` | ro-bind | HTTPS 证书验证 |
| `/etc/hosts` | `/etc/hosts` | ro-bind | hostname 解析 |
| 无 | `/proc` | procfs | 进程信息 |
| 无 | `/dev` | tmpfs + mknod | /dev/null /dev/urandom /dev/tty 等 |
| 无 | `/sys` | sysfs (ro) | 硬件信息，部分工具需要 |
| 无 | `/tmp` | tmpfs | fuse-overlayfs upper/work + 临时文件 |
| `<project_dir>` | `/workspace` | **ro-bind** | 只读原始项目 |
| 无 | `/workspace-rw` | fuse-overlayfs | agent 实际操作路径，写入落到 /tmp/upper |

### 明确不挂载

| 路径 | 原因 |
|------|------|
| `/home` `/root` | 用户私有数据 |
| `/etc/passwd` `/etc/group` | namespace 内用 root 运行，不需要用户映射 |
| `/var` `/run` | 运行时状态，与 agent 无关 |
| `/media` `/mnt` | 外部挂载 |
| `~/.config` `~/.local` | 配置已通过 `--config-json` 传入 |
| `/opt` | 按需通过 extra_binds 配置 |

### 额外 bind（models.toml 配置）

```toml
[[extra_binds]]
host = "/data/shared-knowledge"
target = "/knowledge"
readonly = true

[[extra_binds]]
host = "/home/user/tools"
target = "/tools"
readonly = true
```

通过 `--config-json` 序列化传给 worker，在 `setup_rootfs` 中统一处理。

---

## overlay 生命周期

```
worker 启动
  └─ fuse-overlayfs: lower=/workspace upper=/tmp/upper work=/tmp/work → /workspace-rw
      └─ agent 在 /workspace-rw 工作（所有写入落到 /tmp/upper）

commit
  └─ 遍历 /tmp/upper，将变更 apply 到宿主 project_dir
  └─ 清空 /tmp/upper，重新挂载 overlay（干净状态）

rollback
  └─ 清空 /tmp/upper，重新挂载 overlay（丢弃所有变更）

连接断开
  └─ 进程退出 → mount namespace 销毁 → 内核清理所有 tmpfs/bind/fuse 挂载
  └─ 宿主 project_dir 完全未受影响（除非执行了 commit）
```

**旧的 `set_sandbox` toggle 不再需要**：overlay 是每个 worker 的标配，
始终启用。commit/rollback 替代了 enable/disable 的语义。

---

## 实现文件

| 文件 | 职责 |
|------|------|
| `src/container.rs` | `setup_rootfs(project_dir, extra_binds)` — pre_exec 里执行的纯 libc 代码 |
| `src/server.rs` | `Command::pre_exec(setup_rootfs(...))` + spawn |
| `src/config.rs` | `extra_binds: Vec<BindMount>` 字段加入 Config |
| `models.toml` | `[[extra_binds]]` 配置项 |

---

## 安全边界

- namespace 内是 `uid=0`，但宿主侧仍是普通用户，无法提权
- `/workspace` 只读挂入，agent 代码无法直接修改宿主文件（必须通过 commit）
- 宿主其他目录（`/home` 等）完全不可见
- 网络不隔离（保留 API 访问能力）


## 目标

每个 WebSocket 连接对应一个独立的 worker 子进程，运行在隔离的虚拟 rootfs 中。
project_dir 以 bind 方式挂入容器内的 `/workspace`，agent 的所有操作（文件读写、
shell 命令、Python 脚本等）都只能影响 `/workspace` 及显式挂入的路径，无法触及宿主
其他目录。fuse-overlayfs sandbox 是叠加在 `/workspace` 上的可选层，用于支持 commit/rollback。

---

## 整体架构

```
server 进程（极简 TCP acceptor）
│
├── accept(fd=7) → setup_container() → spawn worker [--worker-fd=7 -d /workspace]
├── accept(fd=8) → setup_container() → spawn worker [--worker-fd=8 -d /workspace]
└── 继续等待连接

  setup_container() 在 pre_exec() 钩子里（fork 后 exec 前，单线程）执行:
    1. unshare(CLONE_NEWUSER | CLONE_NEWNS)
    2. write uid_map / gid_map  → 进程在 namespace 里是 uid=0
    3. mount --bind / /newroot  → 宿主只读基础层（或按路径分开，见下表）
    4. 各路径按表挂载
    5. pivot_root("/newroot", "/newroot/.old") → umount2(".old", MNT_DETACH)
    → exec agent --mode worker ...

worker 进程（在虚拟 rootfs 里运行）
│
├── project_dir = /workspace  （由 -d 参数传入）
├── 可选: set_sandbox → fuse-overlayfs 在 /workspace 上叠加 overlay
│
└── 连接断开 → 进程退出 → namespace 销毁，所有挂载自动清理
```

---

## Mount 表（待决策）

> **图例**：`ro-bind` = 只读绑定；`rw-bind` = 读写绑定；`tmpfs` = 内存文件系统；`proc/dev` = 特殊文件系统

| 宿主路径 | 容器内路径 | 类型 | 原因 | 建议 |
|---------|-----------|------|------|------|
| `/usr` | `/usr` | ro-bind | 所有系统库、Python/Node/gcc 等工具链 | ✅ 必须 |
| `/lib` | `/lib` | ro-bind | glibc 等基础库（部分发行版是 /usr/lib 的 symlink） | ✅ 必须 |
| `/lib64` | `/lib64` | ro-bind | 64位 ld-linux 动态链接器 | ✅ 必须 |
| `/bin` | `/bin` | ro-bind | 基础命令（bash/sh/ls等），许多发行版是 /usr/bin 的 symlink | ✅ 必须 |
| `/sbin` | `/sbin` | ro-bind | fusermount3 等系统工具 | ✅ 必须 |
| `/etc/resolv.conf` | `/etc/resolv.conf` | ro-bind | DNS 解析（API 调用需要） | ✅ 必须 |
| `/etc/ssl/certs` | `/etc/ssl/certs` | ro-bind | HTTPS 证书验证（Anthropic/OpenAI API） | ✅ 必须 |
| `/etc/passwd` | `/etc/passwd` | ro-bind | 用户名查找（部分工具需要） | ⚠️ 可选 |
| `/etc/group` | `/etc/group` | ro-bind | 组信息 | ⚠️ 可选 |
| `/etc/hosts` | `/etc/hosts` | ro-bind | 本地 hostname 解析 | ⚠️ 可选 |
| 无（新建） | `/proc` | proc | 进程信息，部分程序必须 | ✅ 必须 |
| 无（新建） | `/dev` | tmpfs + 节点 | /dev/null /dev/urandom 等，shell 脚本可能用到 | ✅ 必须 |
| 无（新建） | `/tmp` | tmpfs | fuse-overlayfs upper/work 目录放这里；工具临时文件 | ✅ 必须 |
| 无（新建） | `/sys` | sysfs (ro) | 部分工具读取硬件信息 | ⚠️ 可选 |
| `<project_dir>` | `/workspace` | rw-bind | agent 的工作目录 | ✅ 必须 |
| 用户配置 `--bind` | 任意路径 | ro/rw-bind | 全局知识库、共享工具等 | 🔧 可配置 |

**明确不挂载**：

| 路径 | 原因 |
|------|------|
| `/home` | 用户私有数据，agent 不应访问 |
| `/root` | 同上 |
| `/media` `/mnt` | 外部挂载，不需要 |
| `/var` | 运行时状态，不需要 |
| `/run` | 同上 |
| `~/.config` `~/.local` | 个人配置，通过 --config-json 已传入所有需要的配置 |
| `/opt` | 可选软件，如有需要通过 --bind 单独配置 |

---

## Sandbox（fuse-overlayfs）可选层

```
/workspace  (bind from real project_dir)
    ↓ set_sandbox { enabled: true }
/workspace → fuse-overlayfs:
    lower  = /workspace (原始项目)
    upper  = /tmp/agent-sandbox/upper  (写入层，tmpfs)
    work   = /tmp/agent-sandbox/work
    merged = /tmp/agent-sandbox/merged  ← agent 实际操作的路径
```

sandbox 开关是动态的，通过 `set_sandbox` WebSocket 消息控制，不影响容器本身。

---

## 可配置额外 bind mount

`models.toml` 或 server 启动参数支持 `--bind host:target[:ro]`，用于挂载：
- 共享知识库
- 自定义工具目录
- 跨项目公共资源

---

## 生命周期

```
连接建立  → server accept → pre_exec 建立虚拟 rootfs → spawn worker
运行中    → worker 在 /workspace 的 ro/rw 视图下工作
           sandbox 开启时写入只落到 /tmp 的 upper 层
客户端断开 → worker 进程退出 → namespace 随进程销毁（所有 bind/tmpfs 自动 umount）
崩溃      → 进程退出 → 内核自动清理 namespace，无残留
```

与旧方案相比，**不需要手动 fusermount -u**——进程退出时 mount namespace 随之销毁，内核自动清理所有挂载。


## 目标

让 Agent 的所有操作（文件读写、shell 命令、Python 脚本等）都只影响一个隔离的"虚拟视图"，
不修改真实文件系统，直到用户主动 commit。rollback 时真实目录完全不变。

---

## 核心设计：进程级隔离（Process-per-Connection）

### 旧设计问题

```
server 进程
└── tokio task (Agent) ← 与 server 共享文件系统视图
      ├── write_file → overlayfs 隔离 ✅
      └── run_command → 直接 exec，继承真实文件系统 ❌
```

`run_command` 启动的任何子进程以完整进程权限运行，可以访问并修改项目目录以外的任何文件。
路径检查只能拦 `write_file`/`edit_file`，拦不住 shell 命令。

### 新设计

```
server 进程（极简 TCP acceptor）
│
├── accept(fd=7) → spawn worker --fd=7 --project-dir=... [--sandbox]
├── accept(fd=8) → spawn worker --fd=8 ...
└── 继续等待连接（SIGCHLD=SIG_IGN，子进程自动被 OS 回收）

worker 进程（每连接一个，完全自治）
│
├── [若 --sandbox]
│   ├── 创建 /tmp/agent-worker-{uuid}/{upper,work,merged}
│   └── fuse-overlayfs lower=real_project upper=upper → merged
│
├── fd → TcpStream → WebSocket handshake
├── 创建 tokio runtime
├── 运行 Agent (project_dir = merged/)
│
└── 连接断开（TCP EOF）
      → tokio 主循环退出
      → Sandbox::cleanup() 调用 fusermount -u merged
      → 进程退出，OS 回收所有资源
```

---

## 为什么不需要 bwrap / Docker

`bwrap` 本质上只是封装了以下系统调用：
1. `unshare(CLONE_NEWUSER | CLONE_NEWNS)`
2. 写 `/proc/self/uid_map` + `gid_map`
3. 各种 `mount`/`bind`
4. `exec` 目标命令

我们的 worker 进程**自己就是隔离的起点**，不需要外部工具。
工具层的隔离通过 fuse-overlayfs 的 merged 目录实现：
- `write_file` / `edit_file` → 操作 `merged/`，写入落到 `upper/`
- `run_command` → `current_dir(merged/)` 启动，子进程在 merged 视图下工作

---

## 文件系统布局

```
/tmp/agent-worker-{8-char-uuid}/
├── upper/          ← 所有写入（overlayfs upper layer），ext4/tmpfs
├── work/           ← overlayfs 需要的工作目录
└── merged/         ← 合并视图（lower + upper）= agent 看到的项目
                       lower = 真实项目目录（只读）
                       upper = 变更层
```

`/tmp` 是 tmpfs（或 ext4），满足 overlayfs upper 目录的要求。
项目目录可以在任何文件系统上（ext4、exFAT、NFS 等）作为 lower。

---

## 生命周期

```
连接建立  → server accept → spawn worker
初始化    → fuse-overlayfs mount （若 sandbox 开启）
运行中    → agent 完全在 merged/ 视图下工作
客户端断开 → WS EOF → tokio loop 退出 → Sandbox::cleanup() → fusermount -u → 进程退出
崩溃      → 进程退出 → fuse-overlayfs 自动 detach（FUSE 守护进程随 worker 退出）
           /tmp/agent-worker-xxx/ 留下空目录 → server 启动时清理
```

---

## 额外 bind mount（扩展点）

worker 启动时可以通过 `--bind host_path:mount_path[:ro]` 参数挂载额外目录，
供 agent 访问全局知识库、共享工具等。

```bash
agent --mode server --sandbox \
      --bind /home/user/agent_global:/global:ro \
      --bind /home/user/shared_tools:/tools
```

---

## sandbox 开关的行为

| 情形 | 行为 |
|------|------|
| server 启动时 `--sandbox` | 所有连接均启用 sandbox |
| 无 `--sandbox` 启动 | 所有连接直接访问真实 FS（现有行为）|
| 客户端发送 `set_sandbox` | 返回当前 sandbox 状态，不支持运行时切换（需重连）|

> 运行时切换的根本原因：`unshare(CLONE_NEWNS)` 必须在 tokio 多线程 runtime
> 启动之前调用（内核限制：多线程进程不允许 unshare mount namespace）。

---

## 组件变更清单

| 文件 | 变更 |
|------|------|
| `src/server.rs` | 重构为极简 TCP acceptor + worker spawner |
| `src/worker.rs` | 新建：sandbox 初始化 + agent loop（原 handle_connection 逻辑）|
| `src/sandbox.rs` | 新增 `init_for_worker()` + `impl Drop` 自动 cleanup |
| `src/main.rs` | 新增 `RunMode::Worker` + `--worker-fd` / `--sandbox` 参数 |

---

## 未来扩展：rootfs 模式

在 worker 的 namespace 初始化阶段，可以额外挂载一个最小 rootfs：

```
bwrap style（无需 bwrap 工具）:
  pivot_root(rootfs) → 整个文件系统替换为 rootfs
  只 bind /tmp/agent-worker-xxx/merged → /workspace
```

这样 run_command 里的 shell 完全看不到宿主文件系统。
框架已经支持，rootfs 路径作为可选参数传入 worker。

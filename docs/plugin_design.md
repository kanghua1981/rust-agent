# Rust Agent 插件系统设计报告（完整版）

## 1. 设计理念

### 1.1 核心原则
1. **插件是完整功能包**：每个插件包含实现特定功能所需的所有组件（工具、技能、MCP、Hook、资源）
2. **多作用域支持**：支持全局、项目、临时三种插件作用域，满足不同使用场景
3. **Server模式远程管理**：Server模式下支持客户端动态管理插件，提供安全的权限控制
4. **模块化设计**：需要什么功能就安装什么插件，不需要的功能不占用资源

### 1.2 设计目标
- **灵活性**：支持不同作用域的插件，适应各种使用场景
- **安全性**：Server模式下提供完整的插件权限控制
- **性能优化**：按需加载插件，避免资源浪费
- **易于管理**：统一的插件管理接口，清晰的配置层次

## 2. 系统架构

### 2.1 整体架构
```
用户层
├── CLI命令 (agent plugin ...)
├── 交互式命令 (/plugin ...)
├── WebSocket API (Server模式)
└── 配置文件 (多层级配置)

核心层
├── 插件管理器 (PluginManager)
│   ├── 全局作用域管理器 (GlobalScope)
│   ├── 项目作用域管理器 (ProjectScope)
│   └── 临时作用域管理器 (TemporaryScope)
├── 缓存系统 (CacheSystem)
├── Hook执行器 (HookExecutor)
├── 工具注册器 (ToolRegistrar)
└── 权限控制器 (PermissionController)

数据层
├── 全局插件目录 (~/.rust-agent/plugins/)
├── 项目插件目录 (.agent/plugins/)
├── 临时插件存储 (内存)
├── 配置系统 (多层级)
└── 缓存目录 (各作用域独立)
```

### 2.2 插件作用域设计

#### 2.2.1 三种作用域
```rust
pub enum PluginScope {
    Global,     // 用户级别，所有项目共享
    Project,    // 项目级别，仅当前项目使用
    Temporary,  // 会话级别，内存中不持久化
    // Server模式中还有 Session 作用域
}

impl PluginScope {
    pub fn priority(&self) -> u8 {
        match self {
            PluginScope::Temporary => 0,  // 最高优先级
            PluginScope::Project => 1,
            PluginScope::Global => 2,     // 最低优先级
        }
    }
    
    pub fn directory(&self, base_dirs: &BaseDirectories) -> PathBuf {
        match self {
            PluginScope::Global => base_dirs.global_plugin_dir.clone(),
            PluginScope::Project => base_dirs.project_dir.join(".agent/plugins"),
            PluginScope::Temporary => base_dirs.temp_dir.clone(),
        }
    }
}
```

#### 2.2.2 目录结构
```
# 1. 全局插件（用户级别）
~/.rust-agent/
├── plugins/                    # 全局插件目录
│   ├── git-tools/             # Git工具插件（所有项目可用）
│   ├── code-review/           # 代码审查插件
│   ├── docker-tools/          # Docker工具插件
│   └── file-utils/            # 文件工具插件
├── config.toml                # 用户全局配置
└── cache/                     # 全局缓存

# 2. 项目插件（项目级别）
/path/to/project/
├── .agent/
│   ├── plugins/               # 项目插件目录（可选）
│   │   ├── project-tools/     # 项目特定工具
│   │   ├── legacy-scripts/    # 项目遗留脚本
│   │   └── team-shared/       # 团队共享配置
│   ├── plugins.toml           # 项目启用的插件列表
│   ├── config.toml            # 项目配置
│   └── cache/                 # 项目缓存
└── (项目文件)

# 3. 临时插件（会话级别）
内存中存储，不持久化到磁盘
```

#### 2.2.3 加载优先级
插件按作用域优先级加载，高优先级覆盖低优先级：
1. **临时插件**（最高优先级）：会话级别，内存中
2. **项目插件**：项目级别，`.agent/plugins/`
3. **全局插件**（最低优先级）：用户级别，`~/.rust-agent/plugins/`

**冲突解决规则**：
- 同名插件：高优先级作用域的插件生效
- 同名工具：都保留，但需要使用全名调用（`tool@plugin`）
- 配置冲突：高优先级配置覆盖低优先级配置

### 2.3 Server模式插件管理

#### 2.3.1 Server架构
```
Agent Server
├── 全局插件管理器 (所有客户端共享基础)
├── 会话管理器 (管理所有客户端会话)
│   ├── 会话1 (ClientSession)
│   │   ├── 插件管理器 (会话独立)
│   │   ├── 工具执行器 (包含会话插件工具)
│   │   └── 权限控制器 (客户端权限)
│   ├── 会话2
│   └── ...
└── WebSocket处理器 (处理客户端消息)
```

#### 2.3.2 会话隔离设计
```rust
pub struct AgentServer {
    config: Arc<ServerConfig>,
    global_plugins: Arc<PluginManager>,      // 全局插件（只读共享）
    session_manager: SessionManager,         // 会话管理
}

pub struct ClientSession {
    id: String,
    client_info: ClientInfo,
    permission_level: PermissionLevel,
    plugin_manager: SessionPluginManager,    // 会话专用插件管理
    enabled_plugins: HashSet<String>,        // 会话启用的插件
    tool_executor: ToolExecutor,             // 包含会话插件工具
}

impl ClientSession {
    pub async fn enable_plugin(&mut self, plugin_name: &str) -> Result<()> {
        // 1. 检查权限
        if !self.can_enable_plugin(plugin_name) {
            return Err(anyhow!("Insufficient permissions to enable plugin: {}", plugin_name));
        }
        
        // 2. 检查插件是否可用（在全局或项目插件中）
        let plugin_ref = self.find_available_plugin(plugin_name)?;
        
        // 3. 启用插件到会话
        self.plugin_manager.enable(plugin_ref.clone())?;
        self.enabled_plugins.insert(plugin_name.to_string());
        
        // 4. 重新生成会话缓存
        self.regenerate_cache()?;
        
        // 5. 通知客户端
        self.send_plugin_status(plugin_name, "enabled").await;
        
        Ok(())
    }
    
    pub async fn handle_tool_execution(&mut self, tool_name: &str, params: &serde_json::Value) -> Result<ToolResult> {
        // 使用会话的插件工具执行
        // 包含会话启用的所有插件工具
        self.tool_executor.execute(tool_name, params).await
    }
}
```

## 3. 插件定义

### 3.1 插件目录结构（保持现有约定）
```
plugin-name/
├── plugin.toml                      # 插件元数据（必需）
├── README.md                        # 插件说明文档
│
├── mcp/                             # MCP服务器配置（可选）
│   ├── server1.toml
│   └── server2.toml
│
├── tools/                           # 工具脚本（可选）
│   ├── tool1.json                   # 工具定义（JSON Schema）
│   ├── tool1.sh                     # 工具脚本
│   └── ...
│
├── skills/                          # 技能文档（可选）
│   ├── skill1.md
│   └── ...
│
├── hooks/                           # Hook脚本（可选）
│   ├── hook1.py
│   └── ...
│
├── resources/                       # 资源文件（可选）
│   ├── templates/
│   ├── data/
│   └── config/
│
└── examples/                        # 使用示例（可选）
    └── ...
```

### 3.2 插件元数据格式 (plugin.toml)
```toml
# 必需：插件元数据
[metadata]
name = "git-tools"                    # 插件标识符（唯一）
version = "1.0.0"                     # 语义化版本
description = "Git version control tools and automation"
author = "Rust Agent Team <team@rust-agent.dev>"
license = "MIT"
repository = "https://github.com/rust-agent/git-tools-plugin"

# 可选：插件权限级别（用于Server模式安全控制）
[metadata.permissions]
level = "standard"                    # safe, standard, dangerous
requires_approval = false             # 启用时需要客户端确认
max_instances = 1                     # 最大并发实例数

# 可选：依赖管理
[dependencies]
file-utils = ">=1.0.0"                # 依赖的其他插件
code-review = ">=0.5.0"               # 可选依赖

# 可选：系统要求
[system.requirements]
git = ">=2.30.0"
bash = ">=4.0"

# 可选：默认配置
[config]
auto_commit = true
commit_template = "feat: ${description}"
branch_prefix = "agent/"

# 可选：组件清单（可自动扫描）
[components]
scan_directories = ["mcp", "tools", "skills", "hooks"]
```

### 3.3 插件权限模型

#### 3.3.1 权限级别
```rust
pub enum PluginPermissionLevel {
    Safe,       // 安全插件：只读操作，无副作用
    Standard,   // 标准插件：文件操作，需要基本权限
    Dangerous,  // 危险插件：系统命令，网络访问，需要明确授权
}

pub struct PluginPermissions {
    level: PluginPermissionLevel,
    requires_approval: bool,      // 是否需要客户端确认
    allowed_actions: Vec<Action>, // 允许的操作类型
    resource_limits: ResourceLimits, // 资源限制
}

pub enum Action {
    ReadFile,      // 读取文件
    WriteFile,     // 写入文件
    ExecuteCommand, // 执行命令
    NetworkAccess,  // 网络访问
    SystemCall,    // 系统调用
}
```

#### 3.3.2 客户端权限级别
```rust
pub enum ClientPermissionLevel {
    ReadOnly,   // 只读：只能使用安全插件
    Standard,   // 标准：可以使用标准插件
    Admin,      // 管理员：可以使用所有插件
}

impl ClientPermissionLevel {
    pub fn can_use_plugin(&self, plugin: &PluginMeta) -> bool {
        match (self, &plugin.permissions.level) {
            (ClientPermissionLevel::ReadOnly, PluginPermissionLevel::Safe) => true,
            (ClientPermissionLevel::Standard, PluginPermissionLevel::Safe | PluginPermissionLevel::Standard) => true,
            (ClientPermissionLevel::Admin, _) => true,
            _ => false,
        }
    }
}
```

## 4. 插件管理器设计

### 4.1 多作用域管理器
```rust
pub struct PluginManager {
    scopes: HashMap<PluginScope, ScopeManager>,
    enabled_plugins: Vec<PluginRef>,  // 包含作用域信息
    cache_system: CacheSystem,
}

pub struct ScopeManager {
    scope: PluginScope,
    plugin_dir: PathBuf,
    plugins: HashMap<String, PluginMeta>,
    enabled: HashSet<String>,
}

impl PluginManager {
    /// 加载指定作用域的插件
    pub fn load_scopes(&mut self, scopes: Vec<PluginScope>) -> Result<()> {
        for scope in scopes {
            let scope_manager = self.scopes.get_mut(&scope).unwrap();
            let plugins = scope_manager.scan_and_load()?;
            
            // 添加到启用列表（按优先级）
            for plugin in plugins {
                if scope_manager.is_enabled(&plugin.name) {
                    self.enabled_plugins.push(PluginRef {
                        name: plugin.name.clone(),
                        scope: scope.clone(),
                        meta: plugin,
                    });
                }
            }
        }
        
        // 按作用域优先级排序
        self.enabled_plugins.sort_by(|a, b| a.scope.priority().cmp(&b.scope.priority()));
        
        // 生成缓存
        self.generate_cache()
    }
    
    /// 启用插件到指定作用域
    pub fn enable_plugin(&mut self, plugin_name: &str, scope: PluginScope) -> Result<()> {
        let scope_manager = self.scopes.get_mut(&scope).unwrap();
        
        // 检查插件是否存在
        if !scope_manager.has_plugin(plugin_name) {
            return Err(anyhow!("Plugin not found in {} scope: {}", scope, plugin_name));
        }
        
        // 启用插件
        scope_manager.enable(plugin_name)?;
        
        // 更新启用列表
        self.update_enabled_list()?;
        
        // 重新生成缓存
        self.regenerate_cache()
    }
    
    /// 获取所有可用工具（包含作用域信息）
    pub fn get_tools(&self) -> Vec<ToolDefinition> {
        let mut tools = Vec::new();
        
        for plugin_ref in &self.enabled_plugins {
            let plugin_tools = plugin_ref.meta.tools.clone();
            for mut tool in plugin_tools {
                // 添加完整名称：tool@plugin (scope)
                tool.name = format!("{}@{}", tool.name, plugin_ref.name);
                tool.metadata.insert("scope".to_string(), plugin_ref.scope.to_string());
                tool.metadata.insert("plugin".to_string(), plugin_ref.name.clone());
                tools.push(tool);
            }
        }
        
        tools
    }
}
```

### 4.2 缓存系统设计

#### 4.2.1 缓存目录结构
```
# 全局缓存
~/.rust-agent/cache/
├── plugins.json                    # 全局插件索引
├── dependencies.json               # 依赖关系图
└── hashes/                         # 插件内容哈希
    ├── git-tools.sha256
    └── code-review.sha256

# 项目缓存
.agent/cache/
├── manifest.json                   # 缓存清单
├── tools/                          # 合并后的工具
│   ├── git-status@git-tools.json
│   └── lint@code-review.json
├── skills/                         # 合并后的技能
│   ├── git-basics@git-tools.md
│   └── code-standards@code-review.md
├── mcp/                            # 合并后的MCP配置
│   ├── github@git-tools.toml
│   └── eslint@code-review.toml
├── hooks/                          # 合并后的Hook配置
│   └── hooks.toml
└── config/                         # 合并后的配置
    └── merged.toml
```

#### 4.2.2 缓存生成策略
```rust
pub struct CacheGenerator {
    project_dir: PathBuf,
    enabled_plugins: Vec<PluginRef>,
}

impl CacheGenerator {
    pub fn generate(&self) -> Result<()> {
        // 1. 生成清单
        let manifest = self.generate_manifest()?;
        
        // 2. 合并工具
        let tools = self.merge_tools()?;
        self.save_tools(&tools)?;
        
        // 3. 合并技能
        let skills = self.merge_skills()?;
        self.save_skills(&skills)?;
        
        // 4. 合并MCP配置
        let mcp_servers = self.merge_mcp()?;
        self.save_mcp(&mcp_servers)?;
        
        // 5. 合并Hook配置
        let hooks = self.merge_hooks()?;
        self.save_hooks(&hooks)?;
        
        // 6. 合并配置
        let config = self.merge_config()?;
        self.save_config(&config)?;
        
        // 7. 保存清单
        self.save_manifest(&manifest)?;
        
        Ok(())
    }
    
    fn generate_manifest(&self) -> Result<CacheManifest> {
        Ok(CacheManifest {
            version: "1.0".to_string(),
            generated_at: Utc::now(),
            plugins: self.enabled_plugins.iter().map(|p| p.name.clone()).collect(),
            scopes: self.enabled_plugins.iter().map(|p| p.scope.to_string()).collect(),
            components: self.count_components(),
            hashes: self.calculate_hashes()?,
        })
    }
}
```

### 4.3 Server模式会话管理

#### 4.3.1 会话插件管理器
```rust
pub struct SessionPluginManager {
    session_id: String,
    base_plugins: Arc<PluginManager>,      // 基础插件（全局+项目）
    session_plugins: HashMap<String, PluginMeta>, // 会话专用插件
    enabled_plugins: HashSet<String>,      // 会话启用的插件
    cache_dir: PathBuf,                    // 会话缓存目录
}

impl SessionPluginManager {
    /// 启用插件到当前会话
    pub fn enable_plugin(&mut self, plugin_name: &str) -> Result<()> {
        // 1. 检查插件是否可用
        let plugin = self.find_available_plugin(plugin_name)?;
        
        // 2. 检查权限
        self.check_permission(&plugin)?;
        
        // 3. 添加到启用列表
        self.enabled_plugins.insert(plugin_name.to_string());
        
        // 4. 如果是临时插件，加载到内存
        if plugin.scope == PluginScope::Temporary {
            self.load_temporary_plugin(&plugin)?;
        }
        
        // 5. 重新生成会话缓存
        self.regenerate_session_cache()?;
        
        Ok(())
    }
    
    /// 获取会话所有工具（包含基础插件和会话插件）
    pub fn get_session_tools(&self) -> Vec<ToolDefinition> {
        let mut tools = Vec::new();
        
        // 基础插件工具
        let base_tools = self.base_plugins.get_tools();
        tools.extend(base_tools);
        
        // 会话插件工具
        for plugin in self.session_plugins.values() {
            for mut tool in plugin.tools.clone() {
                tool.name = format!("{}@{}", tool.name, plugin.name);
                tool.metadata.insert("scope".to_string(), "session".to_string());
                tool.metadata.insert("session_id".to_string(), self.session_id.clone());
                tools.push(tool);
            }
        }
        
        tools
    }
}
```

#### 4.3.2 WebSocket消息处理
```rust
pub struct WebSocketHandler {
    server: Arc<AgentServer>,
    sessions: Arc<RwLock<HashMap<String, ClientSession>>>,
}

impl WebSocketHandler {
    pub async fn handle_message(&self, session_id: &str, message: ClientMessage) -> Result<ServerMessage> {
        let session = self.get_session(session_id).await?;
        
        match message {
            ClientMessage::EnablePlugin { plugin } => {
                // 启用插件
                session.enable_plugin(&plugin).await?;
                
                Ok(ServerMessage::PluginStatus {
                    plugin,
                    status: "enabled".to_string(),
                    tools_added: session.get_new_tools(),
                    skills_added: session.get_new_skills(),
                })
            }
            
            ClientMessage::DisablePlugin { plugin } => {
                // 禁用插件
                session.disable_plugin(&plugin).await?;
                
                Ok(ServerMessage::PluginStatus {
                    plugin,
                    status: "disabled".to_string(),
                    tools_removed: session.get_removed_tools(),
                    skills_removed: session.get_removed_skills(),
                })
            }
            
            ClientMessage::ListPlugins { scope } => {
                // 列出插件
                let plugins = session.list_plugins(scope).await?;
                
                Ok(ServerMessage::PluginList {
                    plugins,
                    scope,
                })
            }
            
            ClientMessage::LoadTemporaryPlugin { data, name } => {
                // 加载临时插件
                let plugin = session.load_temporary_plugin(&data, &name).await?;
                
                Ok(ServerMessage::TemporaryPluginLoaded {
                    name: plugin.name,
                    tools: plugin.tools,
                    skills: plugin.skills,
                })
            }
            
            // ... 其他消息处理
        }
    }
}
```

## 5. 配置管理

### 5.1 多层级配置系统

#### 5.1.1 配置层次结构
```
优先级从低到高：
1. 系统默认配置（编译时内置）
2. 用户全局配置（~/.rust-agent/config.toml）
3. 插件默认配置（plugin.toml中的[config]节）
4. 项目全局配置（.agent/config.toml）
5. 项目插件配置（.agent/config.toml中的[plugins.<name>]）
6. 会话配置（Server模式中的客户端配置）
7. 环境变量（RUST_AGENT_*）
8. 命令行参数
9. Hook动态修改（最高优先级）
```

#### 5.1.2 配置合并示例
```toml
# 1. 插件默认配置
[config]
timeout = 30
headless = true

# 2. 用户全局配置覆盖
[plugins.git-tools]
timeout = 60      # 覆盖：30 → 60

# 3. 项目配置覆盖
[plugins.git-tools]
headless = false  # 覆盖：true → false
branch_prefix = "feature/"  # 新增配置

# 4. 会话配置覆盖（Server模式）
# 通过WebSocket消息动态设置
```

### 5.2 Server模式安全配置

```toml
# server-config.toml
[server]
port = 9527
host = "0.0.0.0"
max_clients = 100
session_timeout = 3600

[server.plugins]
# 默认启用的插件（所有会话基础）
default_enabled = ["git-tools", "code-review", "file-utils"]

# 插件管理权限
allow_client_enable = true           # 允许客户端启用插件
allow_client_disable = true          # 允许客户端禁用插件
allow_temporary_plugins = false      # 是否允许临时插件
max_plugins_per_session = 10         # 每个会话最大插件数

# 插件白名单（安全控制）
whitelist = [
    "git-tools",
    "code-review",
    "file-utils",
    "docker-tools",
    "web-scraping"
]

# 黑名单（明确禁止）
blacklist = [
    "system-commands",
    "network-scanner"
]

[server.security]
# 客户端权限配置
default_client_level = "standard"    # readonly, standard, admin
admin_tokens = ["super-secret-admin-token"]

# 危险插件控制
dangerous_plugins_require = "admin"  # none, confirmation, admin
require_approval_for = ["docker-tools", "web-scraping"]

# 资源限制
max_memory_per_plugin = "512MB"
max_cpu_time_per_plugin = 30
max_file_size = "10MB"

[server.audit]
log_plugin_operations = true
log_file = "/var/log/rust-agent/plugins.log"
retention_days = 30
```

## 6. 插件管理命令

### 6.1 CLI命令（本地模式）
```bash
# 插件安装（指定作用域）
agent plugin install git-tools                     # 安装到默认作用域（全局）
agent plugin install --global git-tools            # 安装到全局作用域
agent plugin install --project custom-tools        # 安装到项目作用域
agent plugin install --temporary ./temp-plugin     # 安装为临时插件

# 插件管理
agent plugin list                                  # 列出所有插件
agent plugin list --global                         # 列出全局插件
agent plugin list --project                        # 列出项目插件
agent plugin list --available                      # 列出可用插件（所有作用域）

agent plugin enable git-tools                      # 启用插件（当前作用域）
agent plugin enable --global git-tools             # 启用全局插件
agent plugin enable --project custom-tools         # 启用项目插件

agent plugin disable git-tools                     # 禁用插件
agent plugin update git-tools                      # 更新插件
agent plugin update --all                          # 更新所有插件

agent plugin info git-tools                        # 显示插件详情
agent plugin search "git"                          # 搜索插件

# 缓存管理
agent plugin cache clear                           # 清除插件缓存
agent plugin cache rebuild                         # 重新生成缓存
agent plugin cache status                          # 显示缓存状态
```

### 6.2 WebSocket API（Server模式）
```json
// 客户端 -> Server：插件管理消息
{
  "type": "plugin_management",
  "action": "enable",
  "plugin": "git-tools",
  "session_id": "client-123",
  "auth_token": "client-token"
}

{
  "type": "plugin_management",
  "action": "disable",
  "plugin": "code-review"
}

{
  "type": "plugin_management",
  "action": "list",
  "scope": "enabled"  // enabled, available, all
}

{
  "type": "plugin_management",
  "action": "load_temporary",
  "name": "experimental-tool",
  "data": "base64 encoded plugin data..."
}

// Server -> 客户端：插件状态响应
{
  "type": "plugin_status",
  "plugin": "git-tools",
  "status": "enabled",
  "tools_added": [
    {
      "name": "git-status@git-tools",
      "description": "Show Git repository status"
    },
    {
      "name": "git-commit@git-tools",
      "description": "Commit changes to Git"
    }
  ],
  "skills_added": [
    {
      "name": "git-basics@git-tools",
      "title": "Git Basics Guide"
    }
  ]
}

{
  "type": "plugin_error",
  "plugin": "dangerous-plugin",
  "error": "Permission denied",
  "code": "PERMISSION_DENIED",
  "required_level": "admin",
  "current_level": "standard"
}

{
  "type": "plugin_list",
  "scope": "available",
  "plugins": [
    {
      "name": "git-tools",
      "version": "1.0.0",
      "description": "Git version control tools",
      "permission_level": "standard",
      "requires_approval": false,
      "enabled": true
    },
    {
      "name": "docker-tools",
      "version": "0.5.0",
      "description": "Docker container management",
      "permission_level": "dangerous",
      "requires_approval": true,
      "enabled": false
    }
  ]
}
```

### 6.3 交互式命令（REPL模式）
```bash
# 在Agent REPL中使用的命令
/plugin list                        # 列出当前启用的插件
/plugin enable git-tools            # 启用插件
/plugin disable code-review         # 禁用插件
/plugin info file-utils             # 显示插件信息
/plugin tools                       # 列出所有可用工具
/plugin skills                      # 列出所有可用技能
/plugin reload                      # 重新加载插件配置
/plugin cache clear                 # 清除插件缓存

# Server模式特有命令
/plugin session list                # 列出会话插件状态
/plugin session enable git-tools    # 启用插件到当前会话
/plugin session disable git-tools   # 从当前会话禁用插件
```

## 7. 安全设计

### 7.1 插件安全机制

#### 7.1.1 权限验证流程
```rust
pub struct SecurityManager {
    config: Arc<SecurityConfig>,
    audit_logger: AuditLogger,
}

impl SecurityManager {
    pub fn check_plugin_permission(
        &self,
        plugin: &PluginMeta,
        client: &ClientInfo,
        action: PluginAction,
    ) -> Result<()> {
        // 1. 检查插件是否在白名单中
        if !self.config.plugin_whitelist.contains(&plugin.name) {
            self.audit_logger.log_denied(&plugin.name, client, "not in whitelist");
            return Err(anyhow!("Plugin not in whitelist: {}", plugin.name));
        }
        
        // 2. 检查插件是否在黑名单中
        if self.config.plugin_blacklist.contains(&plugin.name) {
            self.audit_logger.log_denied(&plugin.name, client, "in blacklist");
            return Err(anyhow!("Plugin in blacklist: {}", plugin.name));
        }
        
        // 3. 检查客户端权限级别
        if !client.permission_level.can_use_plugin(plugin) {
            self.audit_logger.log_denied(&plugin.name, client, "insufficient permissions");
            return Err(anyhow!(
                "Client permission level {} cannot use plugin {} (requires {})",
                client.permission_level,
                plugin.name,
                plugin.permissions.level
            ));
        }
        
        // 4. 检查是否需要确认
        if plugin.permissions.requires_approval && !client.has_approved(&plugin.name) {
            self.audit_logger.log_requires_approval(&plugin.name, client);
            return Err(anyhow!("Plugin requires approval: {}", plugin.name));
        }
        
        // 5. 检查资源限制
        if !self.check_resource_limits(plugin, client) {
            self.audit_logger.log_denied(&plugin.name, client, "resource limits exceeded");
            return Err(anyhow!("Resource limits exceeded for plugin: {}", plugin.name));
        }
        
        // 6. 记录允许操作
        self.audit_logger.log_allowed(&plugin.name, client, action);
        
        Ok(())
    }
}
```

#### 7.1.2 沙箱执行环境
```rust
pub struct PluginSandbox {
    plugin: PluginMeta,
    limits: ResourceLimits,
}

impl PluginSandbox {
    pub async fn execute_tool(&self, tool: &ToolDefinition, params: &serde_json::Value) -> Result<ToolResult> {
        // 1. 创建隔离环境
        let env = self.create_isolated_environment()?;
        
        // 2. 设置资源限制
        self.set_resource_limits(&env)?;
        
        // 3. 限制文件系统访问
        self.restrict_filesystem_access(&env)?;
        
        // 4. 限制网络访问
        self.restrict_network_access(&env)?;
        
        // 5. 执行工具
        let result = self.execute_in_sandbox(tool, params, &env).await;
        
        // 6. 清理环境
        self.cleanup_environment(&env)?;
        
        result
    }
    
    fn create_isolated_environment(&self) -> Result<IsolatedEnvironment> {
        #[cfg(unix)]
        {
            // 使用Linux namespaces进行隔离
            use nix::sched::{unshare, CloneFlags};
            unshare(CloneFlags::CLONE_NEWNS | CloneFlags::CLONE_NEWPID | CloneFlags::CLONE_NEWNET)?;
            
            // 创建临时目录
            let temp_dir = tempfile::tempdir()?;
            
            // 挂载proc等文件系统
            // ...
            
            Ok(IsolatedEnvironment { temp_dir })
        }
        
        #[cfg(windows)]
        {
            // Windows使用Job对象进行隔离
            // ...
            Ok(IsolatedEnvironment::default())
        }
    }
}
```

### 7.2 审计和监控

#### 7.2.1 审计日志
```rust
pub struct AuditLogger {
    config: Arc<AuditConfig>,
}

impl AuditLogger {
    pub fn log_plugin_operation(
        &self,
        operation: PluginOperation,
        plugin: &str,
        client: &ClientInfo,
        result: &Result<()>,
    ) {
        let entry = AuditEntry {
            timestamp: Utc::now(),
            operation,
            plugin: plugin.to_string(),
            client_id: client.id.clone(),
            client_ip: client.ip.clone(),
            user_agent: client.user_agent.clone(),
            success: result.is_ok(),
            error: result.as_ref().err().map(|e| e.to_string()),
        };
        
        // 写入日志文件
        self.write_log_entry(&entry);
        
        // 发送到监控系统（可选）
        if self.config.enable_monitoring {
            self.send_to_monitoring(&entry);
        }
    }
}

pub enum PluginOperation {
    Enable,
    Disable,
    Execute,
    LoadTemporary,
    UnloadTemporary,
}
```

#### 7.2.2 监控指标
```rust
pub struct PluginMetrics {
    enabled_plugins: Gauge,
    plugin_executions: Counter,
    plugin_errors: Counter,
    plugin_execution_time: Histogram,
    plugin_memory_usage: Gauge,
}

impl PluginMetrics {
    pub fn record_plugin_execution(&self, plugin: &str, duration: Duration, success: bool) {
        self.plugin_executions.inc();
        self.plugin_execution_time.observe(duration.as_secs_f64());
        
        if !success {
            self.plugin_errors.inc();
        }
        
        // 添加标签
        self.plugin_executions.with_label_values(&[plugin]).inc();
    }
}
```

## 8. 实施路线图

### 阶段1：基础框架（2-3周）
1. **插件元数据系统**：实现`plugin.toml`解析和验证
2. **多作用域支持**：全局和项目作用域的基本支持
3. **缓存机制**：基本的缓存生成和加载
4. **工具集成**：插件工具的基本执行

### 阶段2：完整功能（2-3周）
1. **Server模式支持**：会话隔离的插件管理
2. **权限系统**：基本的权限控制和验证
3. **WebSocket API**：插件管理的远程接口
4. **配置系统**：多层级的配置合并

### 阶段3：高级特性（2-3周）
1. **安全沙箱**：插件沙箱执行环境
2. **审计监控**：完整的审计日志和监控
3. **性能优化**：缓存优化和懒加载
4. **迁移工具**：从旧系统迁移到新系统

### 阶段4：生态系统（持续）
1. **官方插件仓库**：插件发布和分发平台
2. **开发工具**：插件开发SDK和模板
3. **常用插件**：开发Git、Docker、AWS等常用插件
4. **社区建设**：文档、示例、贡献指南

## 9. 总结

### 9.1 设计优势
1. **灵活性**：支持多作用域，适应各种使用场景
2. **安全性**：完整的权限控制和沙箱执行
3. **性能**：缓存机制和按需加载
4. **可扩展性**：易于添加新的插件类型和功能
5. **易用性**：统一的管理接口和清晰的配置

### 9.2 预期效果
1. **提高开发效率**：丰富的插件生态系统
2. **增强安全性**：可控的插件执行环境
3. **优化资源使用**：按需加载，避免浪费
4. **促进协作**：团队可以共享项目特定插件
5. **支持远程协作**：Server模式的远程插件管理

### 9.3 下一步行动
1. **评审设计**：团队内部评审设计文档
2. **原型开发**：实现核心框架验证设计
3. **编写文档**：用户指南和开发文档
4. **创建示例**：开发示例插件和工具
5. **社区反馈**：收集早期用户反馈

---

**设计版本**: v5.0 (完整版)
**设计日期**: 2024年4月12日
**核心特性**: 多作用域插件 + Server模式远程管理 + 完整安全控制
---
name: Sub-Agent 调用逻辑
description: 详细说明两种子 Agent 调用方式：call_sub_agent (WebSocket) 和 spawn_sub_agent (stdio) 的使用场景、配置和最佳实践
---

# Sub-Agent 调用逻辑

# Sub-Agent 调用逻辑更新

## 概述

Rust Agent 现在支持两种子 Agent 调用机制，为不同场景提供灵活的任务委派方案。

## 两种调用方式对比

### 1. `call_sub_agent` - WebSocket 服务器模式

**特点**：
- 连接到预启动的 WebSocket 服务器
- 适合长期运行的专家服务
- 保持对话状态和上下文
- 需要预先配置和启动服务

**配置示例** (`~/.config/rust_agent/models.toml`)：
```toml
[sub_agents.coder]
port = 9001
role = "executor"

[sub_agents.reviewer]
port = 9002
role = "checker"
```

**调用参数**：
```json
{
  "prompt": "任务描述",
  "server_url": "ws://localhost:9001",
  "target_dir": "可选的工作目录",
  "auto_approve": false
}
```

### 2. `spawn_sub_agent` - Stdio 子进程模式

**特点**：
- 动态创建 stdio 子进程
- 无需预启动服务器
- 适合临时性、一次性任务
- 默认自动批准工具调用 (`auto_approve: true`)
- 用完即销毁，无需维护

**调用参数**：
```json
{
  "prompt": "任务描述",
  "target_dir": "可选的工作目录",
  "auto_approve": true,
  "timeout_secs": 300
}
```

## 技术实现细节

### 角色隔离机制

为了防止无限递归，系统实现了角色隔离：

```rust
// 在 src/tools/mod.rs 中
let agent_role = std::env::var("AGENT_ROLE").unwrap_or_else(|_| "manager".to_string());
if agent_role == "manager" {
    executor.register(Box::new(call_sub_agent::CallSubAgentTool::new(output.clone())));
    executor.register(Box::new(spawn_sub_agent::SpawnSubAgentTool::new(output)));
}
```

- **Manager Agent**：可以调用子 Agent
- **Worker/Sub-Agent**：不能调用其他 Agent，防止递归

### 事件代理机制

两种调用方式都通过主 Agent 的 `AgentOutput` 代理所有事件：

1. **实时日志**：子 Agent 的思考、工具使用等事件以 `[Sub-Agent ...]` 前缀显示
2. **授权转发**：工具确认请求由主 Agent 转发给用户
3. **工作目录隔离**：通过 `target_dir` 限制文件操作范围

### 协议差异

#### `call_sub_agent` (WebSocket)
- 使用现有的 WebSocket 事件协议
- 支持保持连接和状态
- 需要握手和保活机制

#### `spawn_sub_agent` (Stdio)
- 使用 JSON-over-stdio 协议
- 每个事件为独立的 JSON 行
- 子进程自动退出，无需清理

## 使用场景建议

### 使用 `call_sub_agent` 的场景

1. **代码审查专家**：长期运行，保持审查标准和上下文
2. **测试专家**：持续监控测试状态，运行回归测试
3. **架构设计专家**：维护项目架构知识，提供一致性建议
4. **文档专家**：保持文档风格和格式统一

### 使用 `spawn_sub_agent` 的场景

1. **快速代码生成**：生成单个文件或代码片段
2. **单文件修改**：修复 bug、添加功能到特定文件
3. **临时分析任务**：代码复杂度分析、依赖检查
4. **一次性脚本**：数据转换、批量重命名等

## 安全考虑

### 工作目录隔离
始终使用 `target_dir` 参数限制子 Agent 的文件操作范围，防止误操作全局文件。

### 确认机制
- `call_sub_agent` 默认 `auto_approve: false`，重要操作需要用户确认
- `spawn_sub_agent` 默认 `auto_approve: true`，适合低风险任务

### 超时控制
- `call_sub_agent`：内置 10 分钟总超时，60 秒无活动警告，180 秒无活动终止
- `spawn_sub_agent`：可配置超时（默认 300 秒）

## 性能优化

### 资源管理
1. **连接复用**：`call_sub_agent` 可复用 WebSocket 连接
2. **进程池**：考虑未来实现子进程池减少创建开销
3. **内存限制**：监控子 Agent 内存使用，防止资源泄漏

### 监控指标
- 子 Agent 执行时间
- 工具调用次数
- 内存和 CPU 使用率
- 成功/失败率统计

## 故障处理

### 常见问题及解决方案

1. **连接失败** (`call_sub_agent`)
   - 检查服务器是否运行
   - 验证端口和 URL
   - 检查防火墙设置

2. **子进程卡住** (`spawn_sub_agent`)
   - 超时机制自动终止
   - 检查任务复杂度是否过高
   - 考虑增加 `timeout_secs`

3. **权限问题**
   - 确保工作目录可访问
   - 检查文件读写权限
   - 验证命令执行权限

## 未来扩展

### 计划中的功能
1. **负载均衡**：多个相同角色的子 Agent 池
2. **优先级队列**：重要任务优先处理
3. **结果缓存**：相同任务的缓存复用
4. **健康检查**：定期检查子 Agent 状态

### 集成可能性
1. **CI/CD 集成**：作为流水线中的代码审查步骤
2. **IDE 插件**：实时代码建议和修复
3. **监控告警**：代码质量监控和告警

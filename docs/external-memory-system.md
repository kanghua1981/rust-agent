# 外置记忆系统设计文档

## 概述

本文档描述了Rust Agent的外置记忆系统架构。该设计遵循"关注点分离"原则，将记忆功能从Agent核心中解耦，使其成为可插拔、可扩展的外部服务。

## 设计原则

1. **Agent保持轻量**：核心专注于推理、规划和工具执行
2. **记忆专业化**：专门的记忆服务提供高级功能
3. **可插拔架构**：支持多种记忆后端
4. **向后兼容**：现有功能不受影响
5. **渐进式迁移**：可以从本地记忆平滑迁移到外置记忆

## 架构设计

### 三层架构

```
┌─────────────────────────────────────────┐
│           Rust Agent Core               │
│  • 推理引擎                            │
│  • 规划器                              │
│  • 工具执行器                          │
│  • 对话管理                            │
└────────────────┬────────────────────────┘
                 │ MemoryProvider Trait
                 ▼
┌─────────────────────────────────────────┐
│        Memory Provider Layer            │
│  • LocalFileMemory (默认)               │
│  • HttpMemory (外置服务)                │
│  • NullMemory (测试/沙盒)               │
└────────────────┬────────────────────────┘
                 │ 协议适配
                 ▼
┌─────────────────────────────────────────┐
│        External Memory Services         │
│  • Anda Hippocampus                     │
│  • OpenViking                           │
│  • 自定义记忆服务                       │
└─────────────────────────────────────────┘
```

### MemoryProvider Trait

```rust
pub trait MemoryProvider: Send + Sync {
    // ── Formation ──
    fn record_event(&self, event: MemoryEvent);
    fn log_truncation(&self, summary: &str);
    
    // ── Recall ──
    fn recall(&self) -> String;
    fn recall_relevant(&self, query: &str) -> String;
    
    // ── Maintenance ──
    fn flush(&self) -> anyhow::Result<()>;
    fn add_knowledge(&self, fact: &str);
    
    // ── Introspection ──
    fn is_empty(&self) -> bool;
    fn entry_count(&self) -> usize;
    fn knowledge(&self) -> Vec<String>;
    fn file_map(&self) -> Vec<(String, String)>;
    fn session_log(&self) -> Vec<String>;
}
```

## 支持的记忆后端

### 1. LocalFileMemory (默认)
- 基于 `.agent/memory.md` 文件
- 向后兼容现有系统
- 适合个人使用和小型项目

### 2. HttpMemory (外置服务)
- 通过HTTP协议与外部记忆服务通信
- 支持异步事件发送
- 支持结果缓存
- 可配置超时和重试

### 3. NullMemory
- 无操作实现
- 用于测试和沙盒环境
- 性能基准测试

## 配置系统

### 配置文件格式 (.agent/memory.toml)

```toml
# 记忆后端类型: LocalFile, Http, Null
backend = "Http"

# 本地记忆配置 (仅LocalFile后端使用)
max_knowledge = 15
max_file_map = 25
max_session_log = 40
extraction_frequency = 3

# HTTP记忆配置 (仅Http后端使用)
[http]
base_url = "http://localhost:8080"
api_key = "your-api-key"
space_id = "my-workspace"
timeout_secs = 30
async_events = true
batch_size = 10
cache_ttl = 300
```

### 环境变量支持
```bash
export OPENVIKING_CONFIG_FILE=~/.openviking/ov.conf
export ANDA_HIPPOCAMPUS_API_KEY="your-key"
```

## 与外部服务的集成

### 1. 与Anda Hippocampus集成
```toml
backend = "Http"
[http]
base_url = "https://brain.anda.ai"
api_key = "anda-api-key"
space_id = "my-brain-space"
```

**优势**：
- 获得神经科学启发的记忆整合
- 三阶段睡眠周期自动优化记忆
- 完整的认知演化轨迹

### 2. 与OpenViking集成
```toml
backend = "Http"
[http]
base_url = "http://localhost:1933"
api_key = "openviking-token"
space_id = "default"
```

**优势**：
- 文件系统范式的直观管理
- 三层上下文加载节省token
- 可视化检索轨迹

### 3. 自定义记忆服务
```toml
backend = "Http"
[http]
base_url = "http://localhost:3000/api/v1"
api_key = "custom-token"
```

## 实现细节

### 异步事件处理
```rust
impl MemoryProvider for HttpMemory {
    fn record_event(&self, event: MemoryEvent) {
        // 异步发送事件，避免阻塞Agent
        tokio::spawn(async move {
            if let Err(e) = self.send_event_to_service(event).await {
                tracing::warn!("Failed to send event: {}", e);
            }
        });
    }
}
```

### 智能缓存策略
```rust
struct CachedRecall {
    query: String,
    response: String,
    timestamp: Instant,
    ttl: Duration,
}

impl HttpMemory {
    fn recall_relevant_with_cache(&self, query: &str) -> String {
        // 检查缓存
        if let Some(cached) = self.cache.get(query) {
            if cached.timestamp.elapsed() < cached.ttl {
                return cached.response.clone();
            }
        }
        
        // 查询远程服务
        let response = self.query_remote_service(query).await;
        
        // 更新缓存
        self.cache.insert(query, CachedRecall { /* ... */ });
        
        response
    }
}
```

### 错误处理和降级
```rust
fn create_memory_provider(config: &MemoryConfig) -> Arc<dyn MemoryProvider> {
    match config.backend {
        MemoryBackend::Http => {
            match HttpMemory::new(config.http.clone()) {
                Ok(memory) => Arc::new(memory),
                Err(e) => {
                    tracing::warn!("Failed to create HTTP memory: {}, falling back to local", e);
                    Arc::new(LocalFileMemory::load(project_dir))
                }
            }
        }
        // ...
    }
}
```

## 性能考虑

### 1. 延迟优化
- **异步操作**：事件记录不阻塞Agent执行
- **批量发送**：合并多个事件一次性发送
- **本地缓存**：频繁查询的结果缓存

### 2. 带宽优化
- **增量更新**：只发送变化的部分
- **压缩传输**：支持gzip/brotli压缩
- **智能轮询**：按需查询，避免过度请求

### 3. 内存优化
- **连接池**：复用HTTP连接
- **对象池**：复用请求/响应对象
- **流式处理**：大响应流式读取

## 安全考虑

### 1. 认证和授权
- API密钥管理
- JWT令牌支持
- OAuth 2.0集成

### 2. 数据加密
- TLS传输加密
- 端到端加密选项
- 敏感数据脱敏

### 3. 隐私保护
- 选择性记忆存储
- 数据保留策略
- GDPR合规选项

## 迁移策略

### 阶段1：并行运行
```
现有系统 → LocalFileMemory (主)
新系统   → HttpMemory (实验)
```

### 阶段2：功能验证
- 对比两种后端的记忆质量
- 验证外置服务的稳定性
- 收集性能指标

### 阶段3：逐步切换
```
用户组A → LocalFileMemory
用户组B → HttpMemory (Anda Hippocampus)
用户组C → HttpMemory (OpenViking)
```

### 阶段4：完全迁移
- 默认使用外置记忆
- 本地记忆作为回退选项
- 提供迁移工具

## 监控和运维

### 关键指标
```rust
struct MemoryMetrics {
    // 性能指标
    recall_latency: Histogram,
    formation_success_rate: Gauge,
    cache_hit_rate: Gauge,
    
    // 业务指标
    knowledge_entries: Gauge,
    file_access_count: Gauge,
    session_actions: Gauge,
    
    // 错误指标
    connection_errors: Counter,
    timeout_errors: Counter,
    deserialization_errors: Counter,
}
```

### 健康检查
```bash
# 记忆服务健康检查
curl http://memory-service/health

# Agent记忆集成检查
agent --check-memory
```

### 日志记录
```rust
tracing::info!("Memory recall for query: {}", query);
tracing::debug!("Formation event: {:?}", event);
tracing::warn!("Memory service timeout, using cache");
tracing::error!("Failed to connect to memory service: {}", error);
```

## 未来扩展

### 1. 更多协议支持
- **gRPC**：高性能RPC
- **WebSocket**：实时双向通信
- **MQTT**：物联网场景

### 2. 高级功能
- **向量检索**：集成向量数据库
- **语义搜索**：基于嵌入的相似性搜索
- **知识图谱**：图数据库支持

### 3. 部署选项
- **边缘部署**：本地网络服务
- **云服务**：SaaS记忆服务
- **混合部署**：本地缓存+云同步

## 总结

外置记忆系统为Rust Agent带来了以下优势：

1. **架构清晰**：关注点分离，各司其职
2. **功能强大**：可以集成专业记忆服务的高级功能
3. **灵活扩展**：支持多种后端和协议
4. **易于维护**：记忆系统可以独立升级
5. **成本优化**：可以按需使用付费记忆服务

通过渐进式迁移策略，用户可以平滑地从本地记忆过渡到外置记忆，享受更强大的记忆功能而不影响现有工作流程。
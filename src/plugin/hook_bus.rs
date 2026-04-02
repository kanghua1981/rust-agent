//! Hook 事件总线
//!
//! 负责 hook 的注册、注销、事件分发和脚本执行调度。
//!
//! ## 使用模式
//!
//! `HookBus` 通常包裹在 `Arc<HookBus>` 中，在 `PluginManager`、`Agent` 和
//! `ToolExecutor` 之间共享同一实例。内部对 `hooks` 表使用 `RwLock`，所有 emit
//! 方法在调用 `.await` 前会立即释放读锁，保证不持锁等待外部脚本。

use std::collections::HashMap;
use std::path::Path;
use std::sync::RwLock;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::process::Command;
use tokio::time::timeout;

use super::hook_loader::HookDefinition;

// ── 公开类型 ──────────────────────────────────────────────────────────────────

/// 发布给总线的事件
#[derive(Debug, Clone)]
pub struct HookEvent {
    /// 事件名称（如 "agent.start"、"tool.before"）
    pub name: String,
    /// 当前会话 ID（无则传 "none"）
    pub session_id: String,
    /// 事件特定的业务数据
    pub data: Value,
}

impl HookEvent {
    pub fn new(
        name: impl Into<String>,
        session_id: impl Into<String>,
        data: Value,
    ) -> Self {
        Self {
            name:       name.into(),
            session_id: session_id.into(),
            data,
        }
    }
}

/// Intercepting 模式的执行结果
#[derive(Debug)]
pub enum HookResult {
    /// 允许继续（所有 hook 均通过），交给下游（UI、自动审批等）决策
    Continue,
    /// 中止当前操作，携带拒绝原因
    Cancel { reason: String },
    /// 修改参数后继续（将 `params` 合并到原始 input）
    PatchParams { params: Value },
    /// Hook 已代替用户完成审批，直接放行，不再弹 UI
    /// 脚本输出：`{"approved": true, "message": "<optional>"}` 触发此变体
    Approved { message: Option<String> },
}

// ── HookBus ───────────────────────────────────────────────────────────────────

/// Hook 事件总线
///
/// 线程安全：内部使用 `RwLock` 保护 hooks 映射表，通常由多个
/// `Arc<HookBus>` 共享访问。
#[derive(Debug, Default)]
pub struct HookBus {
    /// event_name → 按 priority 排序的 hook 列表
    hooks: RwLock<HashMap<String, Vec<HookDefinition>>>,
}

impl HookBus {
    pub fn new() -> Self {
        Self::default()
    }

    // ── 注册 / 注销 ──────────────────────────────────────────────────────────

    /// 从插件目录扫描 `hooks/*.toml` 并注册（插件加载时调用）。
    pub fn register_plugin_hooks(&self, plugin_id: &str, plugin_path: &Path) {
        let defs = super::hook_loader::load_hooks_from_plugin(plugin_id, plugin_path);
        if defs.is_empty() {
            return;
        }

        let count = defs.len();
        let mut map = self.hooks.write().expect("HookBus RwLock poisoned");
        for hook in defs {
            let event_hooks = map.entry(hook.event.clone()).or_default();
            event_hooks.push(hook);
            event_hooks.sort_by_key(|h| h.priority);
        }
        tracing::info!("HookBus: +{} hook(s) from plugin {}", count, plugin_id);
    }

    /// 注销插件的所有 hook（插件卸载时调用）。
    pub fn unregister_plugin_hooks(&self, plugin_id: &str) {
        let mut map = self.hooks.write().expect("HookBus RwLock poisoned");
        let mut removed = 0usize;
        for event_hooks in map.values_mut() {
            let before = event_hooks.len();
            event_hooks.retain(|h| h.plugin_id != plugin_id);
            removed += before - event_hooks.len();
        }
        map.retain(|_, v| !v.is_empty());
        if removed > 0 {
            tracing::info!("HookBus: -{} hook(s) from plugin {}", removed, plugin_id);
        }
    }

    // ── 统计 ─────────────────────────────────────────────────────────────────

    /// 已注册 hook 总数。
    pub fn total_hooks(&self) -> usize {
        self.hooks
            .read()
            .expect("HookBus RwLock poisoned")
            .values()
            .map(|v| v.len())
            .sum()
    }

    /// 已注册的事件类别数。
    pub fn event_count(&self) -> usize {
        self.hooks
            .read()
            .expect("HookBus RwLock poisoned")
            .len()
    }

    // ── Emit ─────────────────────────────────────────────────────────────────

    /// **Fire-and-forget**：spawn 异步任务后立即返回，不阻塞调用方。
    ///
    /// 用于通知、日志等不需要结果的场景。
    pub fn emit(&self, event: HookEvent) {
        let hooks = self.snapshot_matching(&event.name, &event.data);
        if hooks.is_empty() {
            return;
        }
        tracing::debug!(
            "[hook] event={} matched={} (fire-and-forget)",
            event.name,
            hooks.len()
        );
        let payload = build_payload(&event);
        for hook in hooks {
            let p = payload.clone();
            tokio::spawn(async move {
                if let Err(e) = run_script(&hook, &p).await {
                    tracing::warn!(
                        "[hook] fire-and-forget '{}' failed: {}",
                        hook.description, e
                    );
                }
            });
        }
    }

    /// **Blocking**：按 priority 顺序等待所有匹配 hook 完成后返回。
    ///
    /// 单个 hook 失败只记录 warn，不影响后续 hook 或主流程。
    pub async fn emit_blocking(&self, event: HookEvent) {
        let hooks = self.snapshot_matching(&event.name, &event.data);
        if hooks.is_empty() {
            return;
        }
        tracing::debug!(
            "[hook] event={} matched={} (blocking)",
            event.name,
            hooks.len()
        );
        let payload = build_payload(&event);
        for hook in &hooks {
            let start = std::time::Instant::now();
            match run_script(hook, &payload).await {
                Ok(_) => {
                    tracing::debug!(
                        "[hook] ✓ {} ({:.1}s)",
                        hook.description,
                        start.elapsed().as_secs_f32()
                    );
                }
                Err(e) => {
                    tracing::warn!("[hook] ✗ {} failed: {}", hook.description, e);
                }
            }
        }
    }

    /// **Intercepting**：顺序执行，任意 hook 返回 `cancel=true` 则立即中止。
    ///
    /// - 脚本执行失败 → 安全降级（视为 Continue），不阻断主流程
    /// - 脚本超时     → 同上（视为 Continue）
    /// - stdout 非 JSON / 空 → 视为 Continue
    pub async fn emit_intercepting(&self, event: HookEvent) -> HookResult {
        let hooks = self.snapshot_matching(&event.name, &event.data);
        if hooks.is_empty() {
            return HookResult::Continue;
        }
        tracing::debug!(
            "[hook] event={} matched={} (intercepting)",
            event.name,
            hooks.len()
        );
        let payload = build_payload(&event);

        for hook in &hooks {
            let stdout = match run_script(hook, &payload).await {
                Ok(out) => out,
                Err(e) => {
                    // 脚本失败 → 降级为 allow，不阻断主流程
                    tracing::warn!("[hook] intercepting '{}' failed (allow): {}", hook.description, e);
                    String::new()
                }
            };

            let trimmed = stdout.trim();
            if trimmed.is_empty() {
                continue; // 无输出 → allow
            }

            match serde_json::from_str::<Value>(trimmed) {
                Ok(json) => {
                    if json.get("cancel").and_then(|v| v.as_bool()).unwrap_or(false) {
                        let reason = json
                            .get("reason")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Hook cancelled the operation")
                            .to_string();
                        tracing::info!("[hook] CANCEL '{}': {}", hook.description, reason);
                        return HookResult::Cancel { reason };
                    }

                    if json.get("approved").and_then(|v| v.as_bool()).unwrap_or(false) {
                        let message = json
                            .get("message")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        tracing::info!("[hook] APPROVED '{}': {:?}", hook.description, message);
                        return HookResult::Approved { message };
                    }

                    if let Some(patch) = json.get("patch_params") {
                        tracing::debug!("[hook] patch_params from '{}'", hook.description);
                        return HookResult::PatchParams { params: patch.clone() };
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "[hook] intercepting '{}' stdout not JSON: {} | {:?}",
                        hook.description, e, trimmed
                    );
                }
            }
        }

        HookResult::Continue
    }

    // ── 内部工具 ──────────────────────────────────────────────────────────────

    /// 获取某事件的匹配 hook 快照（持有读锁的时间仅限于 clone 操作）。
    fn snapshot_matching(&self, event_name: &str, data: &Value) -> Vec<HookDefinition> {
        let map = self.hooks.read().expect("HookBus RwLock poisoned");
        match map.get(event_name) {
            Some(hooks) => hooks
                .iter()
                .filter(|h| matches_filter(h, data))
                .cloned()
                .collect(),
            None => Vec::new(),
        }
        // 读锁在这里释放，此后的所有 .await 不持锁
    }
}

// ── 内部函数 ──────────────────────────────────────────────────────────────────

/// 构建注入脚本的 JSON payload。
fn build_payload(event: &HookEvent) -> Value {
    json!({
        "event":      event.name,
        "timestamp":  chrono::Utc::now().to_rfc3339(),
        "session_id": event.session_id,
        "data":       event.data,
    })
}

/// 执行 hook 脚本；通过 `AGENT_EVENT` 注入 payload，返回 stdout 字符串。
async fn run_script(hook: &HookDefinition, payload: &Value) -> anyhow::Result<String> {
    if !hook.script_path.exists() {
        anyhow::bail!("Script not found: {:?}", hook.script_path);
    }

    let payload_str = serde_json::to_string(payload)?;

    let mut cmd = Command::new(&hook.script_path);
    cmd.env("AGENT_EVENT", &payload_str);
    for (k, v) in &hook.env {
        cmd.env(k, v);
    }
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let fut = async {
        let output = cmd.output().await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Exit {}: {}", output.status, stderr.trim());
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    };

    match timeout(Duration::from_secs(hook.timeout_secs), fut).await {
        Ok(result) => result,
        Err(_)     => anyhow::bail!("Script timed out after {}s", hook.timeout_secs),
    }
}

/// Filter 匹配：对 hook.filter 中的所有字段做 AND 验证。
///
/// 规则：
/// - `min_*` → `data[field] >= value` (数值)
/// - `max_*` → `data[field] <= value` (数值)
/// - `*_prefix` → `data[field].starts_with(value)` (字符串)
/// - 其他    → 精确相等
fn matches_filter(hook: &HookDefinition, data: &Value) -> bool {
    let filter = match &hook.filter {
        Some(f) => f,
        None    => return true, // 无 filter → 全部匹配
    };
    let obj = match filter.as_object() {
        Some(o) => o,
        None    => return true,
    };

    for (key, expected) in obj {
        if let Some(field) = key.strip_prefix("min_") {
            if !numeric_ge(&data[field], expected) {
                return false;
            }
        } else if let Some(field) = key.strip_prefix("max_") {
            if !numeric_le(&data[field], expected) {
                return false;
            }
        } else if let Some(field) = key.strip_suffix("_prefix") {
            let actual = data[field].as_str().unwrap_or("");
            let prefix = expected.as_str().unwrap_or("");
            if !actual.starts_with(prefix) {
                return false;
            }
        } else {
            // 精确匹配
            if &data[key] != expected {
                return false;
            }
        }
    }

    true
}

#[inline]
fn numeric_ge(actual: &Value, expected: &Value) -> bool {
    matches!((actual.as_f64(), expected.as_f64()), (Some(a), Some(e)) if a >= e)
}

#[inline]
fn numeric_le(actual: &Value, expected: &Value) -> bool {
    matches!((actual.as_f64(), expected.as_f64()), (Some(a), Some(e)) if a <= e)
}

// ── 单元测试 ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::hook_loader::HookMode;

    #[test]
    fn filter_exact_match() {
        let hook = HookDefinition {
            event:        "test".into(),
            description:  String::new(),
            script_path:  std::path::PathBuf::from("/tmp/noop.sh"),
            mode:         HookMode::FireAndForget,
            filter:       Some(json!({ "tool_name": "run_command" })),
            env:          HashMap::new(),
            timeout_secs: 30,
            priority:     100,
            plugin_id:    "test@0.1".into(),
        };

        assert!(matches_filter(&hook, &json!({ "tool_name": "run_command" })));
        assert!(!matches_filter(&hook, &json!({ "tool_name": "read_file" })));
    }

    #[test]
    fn filter_min_max() {
        let hook = HookDefinition {
            event:        "test".into(),
            description:  String::new(),
            script_path:  std::path::PathBuf::from("/tmp/noop.sh"),
            mode:         HookMode::FireAndForget,
            filter:       Some(json!({ "min_ratio": 0.8, "max_ratio": 0.95 })),
            env:          HashMap::new(),
            timeout_secs: 30,
            priority:     100,
            plugin_id:    "test@0.1".into(),
        };

        assert!(matches_filter(&hook, &json!({ "ratio": 0.85 })));
        assert!(!matches_filter(&hook, &json!({ "ratio": 0.7 })));
        assert!(!matches_filter(&hook, &json!({ "ratio": 0.99 })));
    }

    #[test]
    fn filter_prefix() {
        let hook = HookDefinition {
            event:        "test".into(),
            description:  String::new(),
            script_path:  std::path::PathBuf::from("/tmp/noop.sh"),
            mode:         HookMode::FireAndForget,
            filter:       Some(json!({ "path_prefix": "/src/" })),
            env:          HashMap::new(),
            timeout_secs: 30,
            priority:     100,
            plugin_id:    "test@0.1".into(),
        };

        assert!(matches_filter(&hook, &json!({ "path": "/src/main.rs" })));
        assert!(!matches_filter(&hook, &json!({ "path": "/tests/foo.rs" })));
    }

    #[test]
    fn no_filter_always_matches() {
        let hook = HookDefinition {
            event:        "test".into(),
            description:  String::new(),
            script_path:  std::path::PathBuf::from("/tmp/noop.sh"),
            mode:         HookMode::FireAndForget,
            filter:       None,
            env:          HashMap::new(),
            timeout_secs: 30,
            priority:     100,
            plugin_id:    "test@0.1".into(),
        };

        assert!(matches_filter(&hook, &json!({})));
        assert!(matches_filter(&hook, &json!({ "anything": "ignored" })));
    }
}

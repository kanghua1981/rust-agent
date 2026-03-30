//! Hook 定义加载器
//!
//! 扫描插件目录中的 `hooks/*.toml`，将每个文件解析为一个 `HookDefinition`。
//! 解析失败的文件会记录警告并跳过，不会中止整体加载。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

// ── 公开类型 ──────────────────────────────────────────────────────────────────

/// Hook 执行模式
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookMode {
    /// 异步 spawn，不等结果，不阻塞主流程
    FireAndForget,
    /// 等待脚本执行完成后继续
    Blocking,
    /// 可通过 stdout JSON 中止当前操作或修改参数
    Intercepting,
}

impl HookMode {
    fn from_str(s: &str) -> Self {
        match s {
            "blocking"     => HookMode::Blocking,
            "intercepting" => HookMode::Intercepting,
            _              => HookMode::FireAndForget,
        }
    }
}

/// 解析后的 hook 定义（运行时使用）
#[derive(Debug, Clone)]
pub struct HookDefinition {
    /// 订阅的事件名（如 "agent.start"、"tool.before"）
    pub event: String,
    /// 人类可读描述（用于日志和 /plugin hooks 命令展示）
    pub description: String,
    /// 脚本的绝对路径
    pub script_path: PathBuf,
    /// 执行模式
    pub mode: HookMode,
    /// 过滤条件 — 与 payload.data 字段做匹配，全部满足才触发（可选）
    pub filter: Option<serde_json::Value>,
    /// 注入给脚本进程的额外环境变量
    pub env: HashMap<String, String>,
    /// 超时秒数（默认 30）
    pub timeout_secs: u64,
    /// 同一事件多个 hook 时的执行顺序（小的先执行，默认 100）
    pub priority: i32,
    /// 所属插件 ID（如 "git-tools@1.0.0"）
    pub plugin_id: String,
}

// ── 内部 TOML 反序列化结构 ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct RawHookDef {
    event: String,
    description: Option<String>,
    script: String,
    mode: Option<String>,
    filter: Option<serde_json::Value>,
    #[serde(default)]
    env: HashMap<String, String>,
    timeout: Option<u64>,
    priority: Option<i32>,
}

// ── 公开函数 ──────────────────────────────────────────────────────────────────

/// 从插件目录扫描所有 hook 定义。
///
/// 扫描路径：`{plugin_path}/hooks/*.toml`
/// 每个 `.toml` 文件对应一个 hook，解析失败的文件会打印 `warn` 并跳过。
pub fn load_hooks_from_plugin(plugin_id: &str, plugin_path: &Path) -> Vec<HookDefinition> {
    let hooks_dir = plugin_path.join("hooks");
    if !hooks_dir.is_dir() {
        return Vec::new();
    }

    let entries = match std::fs::read_dir(&hooks_dir) {
        Ok(e)  => e,
        Err(e) => {
            tracing::warn!("HookLoader: failed to read hooks dir {:?}: {}", hooks_dir, e);
            return Vec::new();
        }
    };

    let mut result = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue; // 忽略非 .toml 文件（如脚本子目录）
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(s)  => s,
            Err(e) => {
                tracing::warn!("HookLoader: failed to read {:?}: {}", path, e);
                continue;
            }
        };

        let raw: RawHookDef = match toml::from_str(&content) {
            Ok(r)  => r,
            Err(e) => {
                tracing::warn!("HookLoader: failed to parse {:?}: {}", path, e);
                continue;
            }
        };

        // 脚本路径解析为插件目录的相对路径
        let script_path = plugin_path.join(&raw.script);

        let hook = HookDefinition {
            event:       raw.event,
            description: raw.description.unwrap_or_default(),
            script_path,
            mode:         HookMode::from_str(raw.mode.as_deref().unwrap_or("fire_and_forget")),
            filter:       raw.filter,
            env:          raw.env,
            timeout_secs: raw.timeout.unwrap_or(30),
            priority:     raw.priority.unwrap_or(100),
            plugin_id:    plugin_id.to_string(),
        };

        tracing::debug!(
            "HookLoader: loaded hook event={} plugin={} script={:?}",
            hook.event, plugin_id, hook.script_path
        );

        result.push(hook);
    }

    if !result.is_empty() {
        tracing::info!(
            "HookLoader: {} hook(s) from plugin {}", result.len(), plugin_id
        );
    }

    result
}

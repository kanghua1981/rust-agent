//! 通道（Channel）扩展点
//!
//! 让插件系统能够声明和管理外部适配器进程（如 WeChat Bridge、Telegram Bot 等）。
//! 通道进程由 PluginManager 根据 `[[channels]]` 声明自动启动/停止，
//! 生命周期依附于 Agent Server。

use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::time;

/// 通道配置（从 plugin.toml 的 `[[channels]]` 段反序列化）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    /// 通道名称（唯一标识，如 "wechat-gateway"）
    pub name: String,
    /// 通道类型：目前仅支持 "process"
    #[serde(default = "default_channel_type")]
    pub r#type: String,
    /// 可执行文件名或路径（相对于插件 bin/ 目录）
    pub command: String,
    /// 命令行参数。支持 `${AGENT_PORT}` 等变量替换。
    #[serde(default)]
    pub args: Vec<String>,
    /// 工作目录（相对于插件目录，或绝对路径）
    #[serde(default)]
    pub working_dir: Option<String>,
    /// 环境变量。支持 `${WECHAT_TOKEN}` 等变量替换。
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Server 启动时是否自动拉起
    #[serde(default = "default_true")]
    pub auto_start: bool,
    /// 进程退出后是否自动重启
    #[serde(default)]
    pub restart_on_exit: bool,
    /// 重启延迟（秒）
    #[serde(default = "default_restart_delay")]
    pub restart_delay_secs: u64,
    /// 所属插件 ID（由 PluginManager 填充）
    #[serde(skip)]
    pub plugin_id: String,
}

fn default_channel_type() -> String {
    "process".to_string()
}

fn default_true() -> bool {
    true
}

fn default_restart_delay() -> u64 {
    5
}

/// 通道子进程状态
#[derive(Debug)]
struct ChannelChild {
    config: ChannelConfig,
    process: Option<Child>,
    restart_count: u64,
    max_restarts: u64,
}

/// 通道进程管理器
///
/// 负责启动、监控和停止所有插件声明的通道子进程。
pub struct ChannelManager {
    children: Vec<ChannelChild>,
    port: u16,
}

impl ChannelManager {
    /// 创建通道管理器
    pub fn new(port: u16) -> Self {
        Self {
            children: Vec::new(),
            port,
        }
    }

    /// 从一组 ChannelConfig 注册并启动 auto_start 的通道
    pub fn spawn_all(&mut self, configs: Vec<ChannelConfig>) {
        for cfg in configs {
            if cfg.auto_start {
                self.spawn_one(cfg);
            } else {
                // 仅注册，不启动（稍后可手动启动）
                self.children.push(ChannelChild {
                    config: cfg,
                    process: None,
                    restart_count: 0,
                    max_restarts: 10,
                });
            }
        }
    }

    /// 启动单个通道进程
    fn spawn_one(&mut self, cfg: ChannelConfig) {
        let args = substitute_vars(&cfg.args, self.port);
        let env_vars: HashMap<String, String> = cfg
            .env
            .iter()
            .map(|(k, v)| (k.clone(), substitute_str(v, self.port)))
            .collect();

        let mut cmd = Command::new(&cfg.command);
        cmd.args(&args);
        if let Some(ref wd) = cfg.working_dir {
            cmd.current_dir(wd);
        }
        for (k, v) in &env_vars {
            cmd.env(k, v);
        }
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
        cmd.stdin(Stdio::null());

        match cmd.spawn() {
            Ok(child) => {
                let pid = child.id();
                println!(
                    "📡 Channel [{}] started (pid {})",
                    cfg.name,
                    pid
                );
                self.children.push(ChannelChild {
                    config: cfg,
                    process: Some(child),
                    restart_count: 0,
                    max_restarts: 10,
                });
            }
            Err(e) => {
                eprintln!(
                    "⚠️  Channel [{}] failed to start: {}",
                    cfg.name, e
                );
                // 仍然注册，以便 restart_on_exit 逻辑可以重试
                self.children.push(ChannelChild {
                    config: cfg,
                    process: None,
                    restart_count: 0,
                    max_restarts: 10,
                });
            }
        }
    }

    /// 停止所有通道子进程
    pub fn stop_all(&mut self) {
        for child in &mut self.children {
            if let Some(ref mut proc) = child.process {
                let pid = proc.id();
                let name = &child.config.name;
                println!("🛑 Stopping channel [{}] (pid {})...", name, pid);
                let _ = proc.kill();
                let _ = proc.wait();
            }
        }
        self.children.clear();
    }

    /// 启动后台监控任务（每 10 秒检查一次，自动重启崩溃的进程）
    pub fn spawn_watchdog(mgr: Arc<Mutex<Self>>) {
        tokio::spawn(async move {
            loop {
                time::sleep(Duration::from_secs(10)).await;
                let mut state = mgr.lock().await;
                // 收集需要重启的通道信息（克隆数据以避免借用冲突）
                let mut to_restart: Vec<(usize, String, String, Vec<String>, Option<String>, u64)> = Vec::new();
                for (i, child) in state.children.iter_mut().enumerate() {
                    let needs_restart = match &mut child.process {
                        Some(proc) => matches!(proc.try_wait(), Ok(Some(_))),
                        None => true,
                    };
                    if needs_restart && child.config.restart_on_exit {
                        if child.restart_count < child.max_restarts {
                            child.restart_count += 1;
                            to_restart.push((
                                i,
                                child.config.name.clone(),
                                child.config.command.clone(),
                                child.config.args.clone(),
                                child.config.working_dir.clone(),
                                child.config.restart_delay_secs,
                            ));
                        } else {
                            eprintln!(
                                "❌ Channel [{}] exceeded max restarts ({}), giving up",
                                child.config.name, child.max_restarts
                            );
                        }
                    }
                }
                let port = state.port;
                drop(state);

                for (_i, name, cmd_str, args, working_dir, delay) in to_restart {
                    eprintln!(
                        "🔄 Channel [{}] exited, restarting in {}s...",
                        name, delay
                    );
                    time::sleep(Duration::from_secs(delay)).await;

                    let mut state = mgr.lock().await;
                    if let Some(target) = state.children.iter_mut().find(|c| c.config.name == name) {
                        let resolved_args: Vec<String> = args
                            .iter()
                            .map(|a| substitute_str(a, port))
                            .collect();
                        let mut cmd = Command::new(&cmd_str);
                        cmd.args(&resolved_args);
                        if let Some(ref wd) = working_dir {
                            cmd.current_dir(wd);
                        }
                        cmd.stdout(Stdio::null());
                        cmd.stderr(Stdio::null());
                        cmd.stdin(Stdio::null());
                        match cmd.spawn() {
                            Ok(proc) => {
                                println!("📡 Channel [{}] restarted (pid {})", name, proc.id());
                                target.process = Some(proc);
                            }
                            Err(e) => {
                                eprintln!("⚠️  Channel [{}] restart failed: {}", name, e);
                            }
                        }
                    }
                }
            }
        });
    }

}

/// 替换参数和环境变量中的 `${AGENT_PORT}` 占位符
fn substitute_vars(args: &[String], port: u16) -> Vec<String> {
    args.iter()
        .map(|a| substitute_str(a, port))
        .collect()
}

fn substitute_str(s: &str, port: u16) -> String {
    s.replace("${AGENT_PORT}", &port.to_string())
}

mod cli;
mod config;
mod confirm;
mod tui_app;
mod context;
mod diff;
mod llm;
mod memory;
mod model_manager;
mod output;
mod persistence;
mod service;
mod skills;
mod streaming;
mod summary;
mod tools;
mod agent;
mod conversation;
mod pipeline;
mod router;
mod sandbox;
mod container;
mod server;
mod ui;
mod worker;
mod workspaces;

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

/// The mode the agent runs in.
#[derive(Debug, Clone, PartialEq)]
pub enum RunMode {
    /// Interactive terminal with colored output (default).
    Cli,
    /// JSON-over-stdio for non-terminal consumers.
    Stdio,
    /// WebSocket server for remote consumers (VS Code, Web UI).
    Server,
    /// Split-screen ratatui TUI: output pane + always-active input bar.
    Tui,
    /// Spawned by the server for each connection; handles the agent loop.
    Worker,
}

impl std::str::FromStr for RunMode {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "cli" => Ok(RunMode::Cli),
            "stdio" => Ok(RunMode::Stdio),
            "server" | "ws" | "websocket" => Ok(RunMode::Server),
            "tui" => Ok(RunMode::Tui),
            "worker" => Ok(RunMode::Worker),
            other => Err(format!("unknown mode '{}', expected: cli, stdio, server, tui, worker", other)),
        }
    }
}

impl std::fmt::Display for RunMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunMode::Cli => write!(f, "cli"),
            RunMode::Stdio => write!(f, "stdio"),
            RunMode::Server => write!(f, "server"),
            RunMode::Tui => write!(f, "tui"),
            RunMode::Worker => write!(f, "worker"),
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "agent", about = "🤖 Rust Coding Agent - An AI-powered CLI coding assistant")]
struct Args {
    /// Optional initial prompt to start with
    #[arg(short, long)]
    prompt: Option<String>,

    /// Model to use (e.g., claude-sonnet-4-20250514, gpt-4o)
    #[arg(short, long, env = "LLM_MODEL", default_value = "claude-sonnet-4-20250514")]
    model: String,

    /// API provider: anthropic, openai, or compatible
    #[arg(long, env = "LLM_PROVIDER", default_value = "anthropic")]
    provider: String,

    /// Working directory (defaults to current directory)
    #[arg(short = 'd', long)]
    workdir: Option<String>,

    /// Enable verbose/debug logging
    #[arg(short, long)]
    verbose: bool,

    /// Skip all confirmation prompts (dangerous!)
    #[arg(short = 'y', long = "yes")]
    auto_approve: bool,

    /// Resume a previous conversation session by ID
    #[arg(short, long)]
    resume: Option<String>,

    /// List saved conversation sessions
    #[arg(long)]
    sessions: bool,

    /// Output mode: cli (default), tui (split-screen ratatui), stdio (JSON protocol), server (WebSocket)
    #[arg(long, default_value = "cli")]
    mode: RunMode,

    /// Host to bind the WebSocket server to (only for --mode server)
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Port for the WebSocket server (only for --mode server)
    #[arg(long, default_value_t = 9527)]
    port: u16,
    /// Maximum iterations for tool usage
    #[arg(long, default_value = "100")]
    max_iterations: usize,

    /// Enable sandbox mode: snapshot files before modification,
    /// allowing /rollback to restore all changes.
    #[arg(long)]
    sandbox: bool,

    /// (Worker mode) Raw socket file descriptor passed from the server process.
    #[arg(long, hide = true)]
    worker_fd: Option<i32>,

    /// (Worker mode) Unique worker instance ID for overlay directory naming.
    #[arg(long, hide = true)]
    worker_id: Option<String>,

    /// (Worker mode) Fully-resolved Config serialized as JSON by the server.
    /// When present, skips Config::load() entirely so the worker does not need
    /// to read models.toml or .env files (safe inside a filesystem sandbox).
    #[arg(long, hide = true)]
    config_json: Option<String>,

    /// (Worker mode) Workspaces list serialized as JSON by the server.
    /// When present, skips workspaces::load() so the worker does not need to
    /// read workspaces.toml from inside a (potentially isolated) container.
    #[arg(long, hide = true)]
    workspaces_json: Option<String>,

    /// Use the global session store (~/.local/share/rust_agent/sessions/) instead of
    /// the project-local `.agent/session.json`. Useful when you want to manage
    /// multiple named sessions across projects.
    #[arg(long)]
    global_session: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env files FIRST, so clap env variables are found during parse.
    // Priority (later wins): ~/.config/rust_agent/.env → ~/.env → CWD chain
    // This ensures a user-level .env in the home directory also works.
    if let Some(home) = dirs::home_dir() {
        // Lowest priority: ~/.config/rust_agent/.env (XDG-style)
        let xdg_env = dirs::config_dir()
            .unwrap_or_else(|| home.join(".config"))
            .join("rust_agent")
            .join(".env");
        dotenvy::from_path(&xdg_env).ok();

        // Medium priority: ~/.env
        dotenvy::from_path(home.join(".env")).ok();
    }
    // Highest priority: .env in CWD or its parents (standard dotenvy behavior)
    dotenvy::dotenv().ok();

    let args = Args::parse();

    // Initialize logging
    let filter = if args.verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("warn")
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    // Set auto-approve if requested
    if args.auto_approve {
        confirm::set_auto_approve(true);
    }

    // Handle --sessions: list and exit
    if args.sessions {
        return cli::list_sessions_and_exit();
    }

    // Load config
    let config = config::Config::load(&args)?;

    // Determine project directory
    let project_dir = if let Some(ref workdir) = args.workdir {
        std::path::PathBuf::from(workdir)
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from(workdir))
    } else {
        std::env::current_dir().unwrap_or_default()
    };

    // TUI mode has its own event loop — launch and return
    if args.mode == RunMode::Tui {
        return tui_app::run(
            config,
            project_dir,
            args.prompt,
            args.resume,
            args.sandbox,
            args.global_session,
            args.auto_approve,
        )
        .await;
    }

    // Auto-spawn sub-agents configured in models.toml (if we're in CLI mode).
    // We keep the Child handles so we can kill the processes when the main agent exits.
    let mut spawned_children: Vec<std::process::Child> = Vec::new();
    if args.mode == RunMode::Cli && !config.sub_agents.is_empty() {
        spawned_children = spawn_sub_agents(&config, &project_dir);
    }

    // Worker mode: spawned by server, handles one connection.
    if args.mode == RunMode::Worker {
        let fd = args.worker_fd.expect("--worker-fd required in worker mode");
        let id = args.worker_id.clone().unwrap_or_else(|| "default".to_string());
        let worker_config = if let Some(ref json) = args.config_json {
            serde_json::from_str(json).unwrap_or(config)
        } else {
            config
        };
        let worker_workspaces: Vec<crate::workspaces::WorkspaceEntry> =
            if let Some(ref json) = args.workspaces_json {
                serde_json::from_str(json).unwrap_or_default()
            } else {
                vec![]
            };
        return worker::run(worker_config, project_dir, fd, args.sandbox, &id, vec![], worker_workspaces).await;
    }

    // Server mode has its own event loop — launch and return
    if args.mode == RunMode::Server {
        return server::run(config, project_dir, &args.host, args.port, args.sandbox).await;
    }

    // Build the output backend based on --mode
    let output: Arc<dyn output::AgentOutput> = match args.mode {
        RunMode::Cli => Arc::new(output::CliOutput::new()),
        RunMode::Stdio => Arc::new(output::StdioOutput::new()),
        RunMode::Server | RunMode::Tui | RunMode::Worker => unreachable!(), // handled above
    };

    // Run the agent
    let result = cli::run(config, project_dir, args.prompt, args.resume, output, args.sandbox, args.global_session).await;

    // Kill auto-spawned sub-agents so they don't become orphan processes.
    // On the next startup the ports would be occupied, causing port-bind failures.
    for mut child in spawned_children {
        let _ = child.kill();
        let _ = child.wait(); // reap to avoid zombie
    }

    result
}

fn spawn_sub_agents(config: &config::Config, project_dir: &std::path::Path) -> Vec<std::process::Child> {
    use std::process::{Command, Stdio};

    let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("agent"));
    let mut children = Vec::new();

    for (name, sa) in &config.sub_agents {
        println!("🚀 Auto-spawning sub-agent '{}' on port {}...", name, sa.port);

        let mut cmd = Command::new(&exe);
        cmd.arg("--mode").arg("server")
           .arg("--port").arg(sa.port.to_string())
           .arg("-d").arg(project_dir)
           .env("AGENT_ROLE", "worker");

        match cmd.stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
            Ok(child) => {
                println!("✅ Sub-agent '{}' started (pid {}).", name, child.id());
                children.push(child);
            }
            Err(e) => eprintln!("❌ Failed to spawn sub-agent '{}': {}", name, e),
        }
    }

    children
}

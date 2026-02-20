mod cli;
mod config;
mod confirm;
mod context;
mod diff;
mod llm;
mod memory;
mod output;
mod persistence;
mod skills;
mod streaming;
mod summary;
mod tools;
mod agent;
mod conversation;
mod server;
mod ui;

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
}

impl std::str::FromStr for RunMode {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "cli" => Ok(RunMode::Cli),
            "stdio" => Ok(RunMode::Stdio),
            "server" | "ws" | "websocket" => Ok(RunMode::Server),
            other => Err(format!("unknown mode '{}', expected: cli, stdio, server", other)),
        }
    }
}

impl std::fmt::Display for RunMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunMode::Cli => write!(f, "cli"),
            RunMode::Stdio => write!(f, "stdio"),
            RunMode::Server => write!(f, "server"),
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
    #[arg(short, long, default_value = "claude-sonnet-4-20250514")]
    model: String,

    /// API provider: anthropic, openai, or compatible
    #[arg(long, default_value = "anthropic")]
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

    /// Output mode: cli (default), stdio (JSON protocol), server (WebSocket)
    #[arg(long, default_value = "cli")]
    mode: RunMode,

    /// Host to bind the WebSocket server to (only for --mode server)
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Port for the WebSocket server (only for --mode server)
    #[arg(long, default_value_t = 9527)]
    port: u16,
    /// Maximum iterations for tool usage
    #[arg(long, default_value = "25")]
    max_iterations: usize,}

#[tokio::main]
async fn main() -> Result<()> {
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

    // Load .env files.
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

    // Server mode has its own event loop — launch and return
    if args.mode == RunMode::Server {
        return server::run(config, project_dir, &args.host, args.port).await;
    }

    // Build the output backend based on --mode
    let output: Arc<dyn output::AgentOutput> = match args.mode {
        RunMode::Cli => Arc::new(output::CliOutput::new()),
        RunMode::Stdio => Arc::new(output::StdioOutput::new()),
        RunMode::Server => unreachable!(), // handled above
    };

    // Run the agent
    cli::run(config, project_dir, args.prompt, args.resume, output).await
}

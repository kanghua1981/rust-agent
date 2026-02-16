mod cli;
mod config;
mod confirm;
mod context;
mod diff;
mod llm;
mod memory;
mod persistence;
mod skills;
mod streaming;
mod summary;
mod tools;
mod agent;
mod conversation;
mod ui;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

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
}

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

    // Load .env if present
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

    // Set working directory
    if let Some(ref workdir) = args.workdir {
        std::env::set_current_dir(workdir)?;
    }

    // Run the agent
    cli::run(config, args.prompt, args.resume).await
}

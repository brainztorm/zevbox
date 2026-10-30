//! Command-line contract (§8.1). Parsing and dispatch only; each command lives
//! in its own file and returns an exit code.

pub mod auth;
mod auth_omlx;
mod clean;
mod doctor;
mod init;
mod launch;
mod logs;
mod setup;
mod shell;
mod status;
mod stop;
mod tools;
mod update;
mod zed;

use crate::supervisor::EXIT_ERROR;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "zevbox",
    version,
    about = "Run Claude / Antigravity / oMLX for Zed inside isolated Apple Container micro-VMs"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AgentArg {
    Claude,
    Antigravity,
    Omlx,
}

impl AgentArg {
    pub fn id(self) -> &'static str {
        match self {
            AgentArg::Claude => "claude",
            AgentArg::Antigravity => "antigravity",
            AgentArg::Omlx => "omlx",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::value_variants()
            .iter()
            .copied()
            .find(|a| a.id() == id)
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Guided first-time setup (safe to re-run)
    Setup,
    /// Create the state directory, config.json and tools.json if missing
    Init,
    /// Check the installation and print fixes
    Doctor {
        #[arg(long)]
        json: bool,
    },
    /// Manage zevbox entries in Zed settings
    Zed {
        #[command(subcommand)]
        action: ZedAction,
    },
    /// Log an agent in, inside its Linux VM (interactive); for omlx: connect to
    /// the local inference server
    Auth {
        agent: AgentArg,
        #[arg(long)]
        status: bool,
        /// omlx only: server URL, e.g. http://192.168.1.8:8000
        #[arg(long)]
        url: Option<String>,
        /// omlx only: default model (default: the smallest one on the server)
        #[arg(long)]
        model: Option<String>,
    },
    /// Called by Zed: start the agent VM and relay ACP on stdin/stdout
    Launch {
        agent: AgentArg,
        /// Project root. Default: the current directory (Zed starts the agent
        /// in the project's first worktree).
        #[arg(long)]
        project: Option<PathBuf>,
    },
    /// Show running zevbox instances
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Stop zevbox instances (all of them if no argument)
    Stop {
        /// Instance id (8 hex characters)
        id: Option<String>,
        #[arg(long, conflicts_with = "id")]
        project: Option<PathBuf>,
    },
    /// Remove leftovers (orphans, old logs/backups; old images with --images)
    Clean {
        #[arg(long)]
        images: bool,
    },
    /// Interactive Linux shell in a throw-away VM identical to the agent's
    Shell {
        agent: AgentArg,
        #[arg(long)]
        project: Option<PathBuf>,
    },
    /// Show the log of an instance (latest by default)
    Logs {
        id: Option<String>,
        #[arg(long)]
        path: bool,
    },
    /// Check for / install new agent versions (build + test + promote)
    Update {
        #[arg(long)]
        check: bool,
        #[arg(long)]
        agent: Option<AgentArg>,
    },
    /// Go back to the previous release
    Rollback {
        #[arg(long)]
        agent: Option<AgentArg>,
    },
    /// Manage extra Linux tools (apt packages) baked into the images
    Tools {
        #[command(subcommand)]
        action: ToolsAction,
    },
    /// Full-screen keyboard menu over the other commands (English or French)
    Console {
        /// Interface language (default: from LC_ALL, LC_MESSAGES or LANG; fr* = French)
        #[arg(long, value_enum)]
        lang: Option<crate::console::i18n::Lang>,
    },
}

#[derive(Debug, Subcommand)]
pub enum ZedAction {
    Install {
        #[arg(long)]
        settings: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
    },
    Uninstall {
        #[arg(long)]
        settings: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
pub enum ToolsAction {
    List,
    Add {
        #[arg(required = true)]
        packages: Vec<String>,
    },
    Remove {
        #[arg(required = true)]
        packages: Vec<String>,
    },
}

/// Entry point used by `main`. Returns the process exit code.
pub fn run() -> u8 {
    let cli = Cli::parse();
    // Opt-in diagnostics for every command except `launch` (which logs to its own
    // file: stdout carries JSON-RPC only and stderr belongs to Zed, I6) and
    // `console` (stderr lines would tear its full screen).
    if std::env::var_os("RUST_LOG").is_some()
        && !matches!(
            cli.command,
            Command::Launch { .. } | Command::Console { .. }
        )
    {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_writer(std::io::stderr)
            .with_ansi(false)
            .with_target(false)
            .try_init();
    }
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            crate::ui::err(&format!("zevbox: cannot start async runtime: {e}"));
            return EXIT_ERROR;
        }
    };
    let code = runtime.block_on(async move {
        match dispatch(cli.command).await {
            Ok(code) => code,
            Err(e) => {
                crate::ui::err(&format!("zevbox: {e:#}"));
                EXIT_ERROR
            }
        }
    });
    runtime.shutdown_background();
    code
}

async fn dispatch(command: Command) -> anyhow::Result<u8> {
    let paths = crate::paths::Paths::from_env()?;
    if !matches!(
        command,
        Command::Init
            | Command::Setup
            | Command::Launch { .. }
            | Command::Auth { .. }
            | Command::Console { .. }
    ) && !paths.config_file().exists()
    {
        crate::ui::err(
            "zevbox is not set up. Fix: run 'zevbox setup' or 'zevbox init' in a macOS terminal.",
        );
        return Ok(crate::supervisor::EXIT_PRECONDITION);
    }

    match command {
        Command::Setup => setup::run().await,
        Command::Init => init::run().await,
        Command::Doctor { json } => doctor::run(json).await,
        Command::Zed { action } => zed::run(action).await,
        Command::Auth {
            agent,
            status,
            url,
            model,
        } => auth::run_with(agent, status, auth::OmlxOptions { url, model }).await,
        Command::Launch { agent, project } => launch::run(agent, project).await,
        Command::Status { json } => status::run(json).await,
        Command::Stop { id, project } => stop::run(id, project).await,
        Command::Clean { images } => clean::run(images).await,
        Command::Shell { agent, project } => shell::run(agent, project).await,
        Command::Logs { id, path } => logs::run(id, path).await,
        Command::Update { check, agent } => update::run_update(check, agent).await,
        Command::Rollback { agent } => update::run_rollback(agent).await,
        Command::Tools { action } => tools::run(action).await,
        // Starts without config.json: « Première installation » runs `setup`.
        Command::Console { lang } => crate::console::run(lang).await,
    }
}

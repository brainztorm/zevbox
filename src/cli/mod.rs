//! Command-line contract (§8.1). Parsing and dispatch only; each command lives
//! in its own file and returns an exit code.

mod auth;
mod clean;
mod doctor;
mod init;
mod launch;
mod logs;
mod project;
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
    about = "Run Claude / Antigravity for Zed inside isolated Apple Container micro-VMs"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AgentArg {
    Claude,
    Antigravity,
}

impl AgentArg {
    pub fn id(self) -> &'static str {
        match self {
            AgentArg::Claude => "claude",
            AgentArg::Antigravity => "antigravity",
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Guided first-time setup (safe to re-run)
    Setup {
        #[arg(long)]
        project: Option<PathBuf>,
    },
    /// Create the state directory, config.json and tools.json if missing
    Init,
    /// Check the installation and print fixes
    Doctor {
        #[arg(long)]
        json: bool,
    },
    /// Manage allowed projects
    Project {
        #[command(subcommand)]
        action: ProjectAction,
    },
    /// Manage zevbox entries in Zed settings
    Zed {
        #[command(subcommand)]
        action: ZedAction,
    },
    /// Log an agent in, inside its Linux VM (interactive)
    Auth {
        agent: AgentArg,
        #[arg(long)]
        status: bool,
    },
    /// Called by Zed: start the agent VM and relay ACP on stdin/stdout
    Launch {
        agent: AgentArg,
        #[arg(long)]
        project: PathBuf,
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
}

#[derive(Debug, Subcommand)]
pub enum ProjectAction {
    Add {
        path: PathBuf,
        #[arg(long)]
        no_zed: bool,
    },
    Remove {
        path: PathBuf,
    },
    List,
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
        Command::Init | Command::Setup { .. } | Command::Launch { .. }
    ) && !paths.config_file().exists()
    {
        crate::ui::err(
            "zevbox is not set up. Fix: run 'zevbox setup' or 'zevbox init' in a macOS terminal.",
        );
        return Ok(crate::supervisor::EXIT_PRECONDITION);
    }

    match command {
        Command::Setup { project } => setup::run(project).await,
        Command::Init => init::run().await,
        Command::Doctor { json } => doctor::run(json).await,
        Command::Project { action } => project::run(action).await,
        Command::Zed { action } => zed::run(action).await,
        Command::Auth { agent, status } => auth::run(agent, status).await,
        Command::Launch { agent, project } => launch::run(agent, project).await,
        Command::Status { json } => status::run(json).await,
        Command::Stop { id, project } => stop::run(id, project).await,
        Command::Clean { images } => clean::run(images).await,
        Command::Shell { agent, project } => shell::run(agent, project).await,
        Command::Logs { id, path } => logs::run(id, path).await,
        Command::Update { check, agent } => update::run_update(check, agent).await,
        Command::Rollback { agent } => update::run_rollback(agent).await,
        Command::Tools { action } => tools::run(action).await,
    }
}

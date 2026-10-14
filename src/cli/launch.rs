//! `zevbox launch` (§8.1, §8.3). stdout belongs to ACP: never use `ui` here (I6).

use super::AgentArg;
use std::path::PathBuf;

pub async fn run(agent: AgentArg, project: Option<PathBuf>) -> anyhow::Result<u8> {
    let paths = crate::paths::Paths::from_env()?;
    // Zed runs custom agent servers with the project's first worktree as the
    // working directory (§8.9), so the current directory is the project. If it
    // cannot be read, preflight reports "not an allowed project" (§8.5).
    let project = match project {
        Some(p) => p,
        None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };
    Ok(crate::supervisor::launch(&paths, agent.id(), &project).await)
}

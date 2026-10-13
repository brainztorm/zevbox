//! `zevbox launch` (§8.1, §8.3). stdout belongs to ACP: never use `ui` here (I6).

use super::AgentArg;
use std::path::PathBuf;

pub async fn run(agent: AgentArg, project: PathBuf) -> anyhow::Result<u8> {
    let paths = crate::paths::Paths::from_env()?;
    Ok(crate::supervisor::launch(&paths, agent.id(), &project).await)
}

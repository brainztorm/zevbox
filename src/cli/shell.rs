//! `zevbox shell` (§8.1).

pub async fn run(
    agent: super::AgentArg,
    project: Option<std::path::PathBuf>,
) -> anyhow::Result<u8> {
    let _ = (agent, project);
    anyhow::bail!("not implemented yet: zevbox shell")
}

//! `zevbox auth` (§8.1).

pub async fn run(agent: super::AgentArg, status: bool) -> anyhow::Result<u8> {
    let _ = (agent, status);
    anyhow::bail!("not implemented yet: zevbox auth")
}

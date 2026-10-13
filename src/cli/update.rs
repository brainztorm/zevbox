//! `zevbox update` and `zevbox rollback` (§8.1, §8.7).

use super::AgentArg;

pub async fn run_update(check: bool, agent: Option<AgentArg>) -> anyhow::Result<u8> {
    let _ = (check, agent);
    anyhow::bail!("not implemented yet: zevbox update")
}

pub async fn run_rollback(agent: Option<AgentArg>) -> anyhow::Result<u8> {
    let _ = agent;
    anyhow::bail!("not implemented yet: zevbox rollback")
}

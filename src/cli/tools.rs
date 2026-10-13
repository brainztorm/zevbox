//! `zevbox tools` (§8.1).

pub async fn run(action: super::ToolsAction) -> anyhow::Result<u8> {
    let _ = (action,);
    anyhow::bail!("not implemented yet: zevbox tools")
}

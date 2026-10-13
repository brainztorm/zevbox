//! `zevbox setup` (§8.1).

pub async fn run(project: Option<std::path::PathBuf>) -> anyhow::Result<u8> {
    let _ = (project,);
    anyhow::bail!("not implemented yet: zevbox setup")
}

//! `zevbox stop` (§8.1).

pub async fn run(id: Option<String>, project: Option<std::path::PathBuf>) -> anyhow::Result<u8> {
    let _ = (id, project);
    anyhow::bail!("not implemented yet: zevbox stop")
}

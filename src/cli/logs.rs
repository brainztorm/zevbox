//! `zevbox logs` (§8.1).

pub async fn run(id: Option<String>, path: bool) -> anyhow::Result<u8> {
    let _ = (id, path);
    anyhow::bail!("not implemented yet: zevbox logs")
}

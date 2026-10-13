//! `zevbox status` (§8.1).

use crate::paths::Paths;
use crate::supervisor::{EXIT_OK, EXIT_PRECONDITION};
use serde_json::json;

pub async fn run(json: bool) -> anyhow::Result<u8> {
    let paths = Paths::from_env()?;
    if !paths.config_file().exists() {
        crate::ui::err(
            "zevbox is not set up. Fix: run 'zevbox setup' or 'zevbox init' in a macOS terminal.",
        );
        return Ok(EXIT_PRECONDITION);
    }
    let config = crate::config::load(&paths.config_file())?;
    let cli = crate::container::ContainerCli::new(config.container_binary);

    let records = crate::instances::list(&paths).unwrap_or_default();
    let containers = cli.list_all().await.unwrap_or_default();

    let mut items = Vec::new();

    for rec in &records {
        let state = if crate::instances::is_orphan(rec) {
            "orphan"
        } else {
            "running"
        };
        items.push(json!({
            "id": rec.id,
            "state": state,
            "agent": rec.agent,
            "project": rec.project,
            "release": rec.release,
            "image": rec.image,
            "containerName": rec.container_name,
            "relayPid": rec.relay_pid,
        }));
    }

    for c in &containers {
        if crate::container::may_target(c) && !records.iter().any(|r| r.container_name == c.name) {
            let id = c
                .labels
                .get(crate::container::LABEL_INSTANCE)
                .cloned()
                .unwrap_or_default();
            let agent = c
                .labels
                .get(crate::container::LABEL_AGENT)
                .cloned()
                .unwrap_or_default();
            items.push(json!({
                "id": id,
                "state": "unregistered",
                "agent": agent,
                "containerName": c.name,
            }));
        }
    }

    if json {
        crate::ui::out(&serde_json::to_string(&items)?);
    } else if items.is_empty() {
        crate::ui::out("No zevbox instances running.");
    } else {
        crate::ui::out(&format!(
            "{:<10} {:<14} {:<10} {:<24} {:<20}",
            "ID", "STATE", "AGENT", "CONTAINER", "PROJECT"
        ));
        for item in &items {
            let id = item["id"].as_str().unwrap_or("-");
            let state = item["state"].as_str().unwrap_or("-");
            let agent = item["agent"].as_str().unwrap_or("-");
            let name = item["containerName"].as_str().unwrap_or("-");
            let proj = item["project"].as_str().unwrap_or("-");
            crate::ui::out(&format!(
                "{:<10} {:<14} {:<10} {:<24} {:<20}",
                id, state, agent, name, proj
            ));
        }
    }

    Ok(EXIT_OK)
}

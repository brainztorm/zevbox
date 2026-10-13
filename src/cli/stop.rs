//! `zevbox stop` (§8.1).

use crate::paths::Paths;
use crate::supervisor::{EXIT_ERROR, EXIT_OK, EXIT_PRECONDITION};

pub async fn run(id: Option<String>, project: Option<std::path::PathBuf>) -> anyhow::Result<u8> {
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

    if let Some(target_id) = id {
        let Some(rec) = records.iter().find(|r| r.id == target_id) else {
            crate::ui::err(&format!("zevbox: unknown instance id '{target_id}'"));
            return Ok(EXIT_ERROR);
        };
        stop_record(&cli, &paths, rec).await;
        return Ok(EXIT_OK);
    }

    if let Some(target_project) = project {
        let canonical = std::fs::canonicalize(&target_project).unwrap_or(target_project);
        for rec in &records {
            if rec.project == canonical {
                stop_record(&cli, &paths, rec).await;
            }
        }
        return Ok(EXIT_OK);
    }

    // Stop all instances
    for rec in &records {
        stop_record(&cli, &paths, rec).await;
    }

    Ok(EXIT_OK)
}

async fn stop_record(
    cli: &crate::container::ContainerCli,
    paths: &Paths,
    rec: &crate::instances::InstanceRecord,
) {
    let pid = nix::unistd::Pid::from_raw(rec.relay_pid as i32);
    // Send SIGTERM to relay process
    let sent = nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGTERM).is_ok();
    // If relay process is already dead or not reachable, stop container directly
    if !sent || crate::instances::is_orphan(rec) {
        let _ = cli.stop_and_delete_verified(&rec.container_name).await;
        let _ = crate::instances::remove(paths, &rec.id);
    }
}

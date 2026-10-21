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

/// How long a live relay gets, after SIGTERM, to stop its container and remove its
/// record itself (§8.3 allows ~9 s; one more second of margin).
const RELAY_GRACE: std::time::Duration = std::time::Duration::from_secs(10);

/// Stops one instance and says so on stdout (§8.1 « affiche ce qui a été arrêté »).
///
/// Orphan check FIRST: an orphan's `relay_pid` may have been reused by an unrelated
/// process of the user (typical after a reboot), and SIGTERM must never reach it.
/// A live relay gets SIGTERM and [`RELAY_GRACE`] to clean up; if it does not, the
/// container is stopped and deleted directly (verified `zevbox-*` + label, I8).
async fn stop_record(
    cli: &crate::container::ContainerCli,
    paths: &Paths,
    rec: &crate::instances::InstanceRecord,
) {
    let mut how = "orphan";
    if !crate::instances::is_orphan(rec) {
        let pid = nix::unistd::Pid::from_raw(rec.relay_pid as i32);
        how = if nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGTERM).is_ok() {
            "relay stopped"
        } else {
            "relay unreachable"
        };
        let deadline = tokio::time::Instant::now() + RELAY_GRACE;
        while tokio::time::Instant::now() < deadline
            && !crate::instances::is_orphan(rec)
            && paths.instance_file(&rec.id).exists()
        {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        if crate::instances::is_orphan(rec) || !paths.instance_file(&rec.id).exists() {
            // The relay did its own cleanup (or died: the checks below finish the job).
        } else {
            how = "relay did not answer, container stopped directly";
        }
    }
    // Idempotent: no-op if the container is already gone.
    let _ = cli.stop_and_delete_verified(&rec.container_name).await;
    let _ = crate::instances::remove(paths, &rec.id);
    crate::ui::out(&format!(
        "stopped {} ({}, {how})",
        rec.id, rec.container_name
    ));
}

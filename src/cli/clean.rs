//! `zevbox clean` (§8.1).

use crate::paths::Paths;
use crate::supervisor::{EXIT_OK, EXIT_PRECONDITION};

pub async fn run(images: bool) -> anyhow::Result<u8> {
    let paths = Paths::from_env()?;
    if !paths.config_file().exists() {
        crate::ui::err(
            "zevbox is not set up. Fix: run 'zevbox setup' or 'zevbox init' in a macOS terminal.",
        );
        return Ok(EXIT_PRECONDITION);
    }
    let config = crate::config::load(&paths.config_file())?;
    let cli = crate::container::ContainerCli::new(config.container_binary);

    // 1. Clean orphan instance records and their containers
    let cleaned = crate::supervisor::clean_orphans(&paths).await;
    for id in cleaned {
        crate::ui::out(&format!("cleaned orphan instance {id}"));
    }

    // 2. Clean unregistered zevbox containers of this state root only (backlog 10)
    let records = crate::instances::list(&paths).unwrap_or_default();
    if let Ok(containers) = cli.list_ours(&paths.state_root).await {
        for c in containers {
            if !records.iter().any(|r| r.container_name == c.name) {
                crate::ui::out(&format!("removing unregistered container {}", c.name));
                let _ = cli.stop_and_delete_verified(&c.name).await;
            }
        }
    }

    // 3. Prune old logs and backups
    let _ = crate::fsutil::prune_oldest(&paths.logs_dir(), ".log", 30);
    let _ = crate::fsutil::prune_oldest(&paths.backups_dir(), ".json", 20);

    // 4. Old release images and the image builder VM (DECISIONS G1)
    if images {
        for reference in crate::release::prune_images(&paths, &cli).await {
            crate::ui::out(&format!("removed old image {reference}"));
        }
        prune_release_files(&paths);
    }

    Ok(EXIT_OK)
}

/// Old release folders and build archives (DECISIONS R4), with a line per result.
/// Also used by `update` after every run.
pub fn prune_release_files(paths: &Paths) {
    let (deleted, freed) = crate::release::prune_release_dirs(paths);
    for name in deleted {
        crate::ui::out(&format!("removed old release folder {name}"));
    }
    if freed > 0 {
        crate::ui::out(&format!(
            "freed {:.0} MB of build archives",
            freed as f64 / 1e6
        ));
    }
}

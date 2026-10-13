//! `zevbox setup` (§8.1).

use crate::paths::Paths;
use crate::supervisor::{EXIT_ERROR, EXIT_OK};
use std::path::PathBuf;

pub async fn run(project: Option<PathBuf>) -> anyhow::Result<u8> {
    let paths = Paths::from_env()?;
    let config_existed = paths.config_file().exists();

    if !config_existed {
        crate::ui::out("initializing zevbox configuration...");
        let code = crate::cli::init::run().await?;
        if code != EXIT_OK {
            return Ok(code);
        }
    } else {
        crate::ui::out("zevbox already initialized (keeping existing config)");
    }

    let mut config = crate::config::load(&paths.config_file())?;

    if let Some(p) = project {
        let canon = std::fs::canonicalize(&p).unwrap_or(p.clone());
        if !config.projects.contains(&canon) {
            crate::ui::out(&format!("adding project {}", canon.display()));
            let code = crate::cli::project::run(super::ProjectAction::Add {
                path: p,
                no_zed: false,
            })
            .await?;
            if code != EXIT_OK {
                return Ok(code);
            }
            config = crate::config::load(&paths.config_file())?;
        } else {
            crate::ui::out(&format!("project {} already registered", canon.display()));
        }
    }

    crate::ui::out("synchronizing Zed settings...");
    let _ = crate::cli::zed::run(super::ZedAction::Install {
        settings: None,
        dry_run: false,
    })
    .await;

    // Check Apple Container runtime status
    let cli = crate::container::ContainerCli::new(config.container_binary.clone());
    if let Ok(sys) = cli.system_status().await {
        if !sys.running {
            crate::ui::out("starting Apple Container runtime...");
            let _ = cli.system_start().await;
        }
    }

    crate::ui::out("running diagnostics...");
    let doc_code = crate::cli::doctor::run(false).await?;
    if doc_code == EXIT_ERROR {
        crate::ui::err("doctor reported errors during setup");
    }

    crate::ui::out("zevbox setup complete");
    Ok(EXIT_OK)
}

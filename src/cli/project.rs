//! `zevbox project` (§8.1).

use crate::paths::Paths;
use crate::supervisor::{EXIT_ERROR, EXIT_OK, EXIT_PRECONDITION};

pub async fn run(action: super::ProjectAction) -> anyhow::Result<u8> {
    let paths = Paths::from_env()?;
    if !paths.config_file().exists() {
        crate::ui::err(
            "zevbox is not set up. Fix: run 'zevbox setup' or 'zevbox init' in a macOS terminal.",
        );
        return Ok(EXIT_PRECONDITION);
    }

    match action {
        super::ProjectAction::Add { path, no_zed } => {
            let mut config = crate::config::load(&paths.config_file())?;
            let installed_bin = crate::paths::installed_binary()?;
            let rules = crate::config::ProjectRules {
                home: crate::paths::home_dir()?,
                forbidden: vec![
                    paths.state_root.clone(),
                    installed_bin
                        .parent()
                        .unwrap_or(std::path::Path::new(""))
                        .to_path_buf(),
                ],
            };
            let canonical = match crate::config::validate_project_path(&path, &rules) {
                Ok(c) => c,
                Err(e) => {
                    crate::ui::err(&format!("zevbox: {e}"));
                    return Ok(EXIT_ERROR);
                }
            };
            if !config.projects.contains(&canonical) {
                config.projects.push(canonical);
                crate::config::save(&paths.config_file(), &config)?;
            }
            if !no_zed {
                let _ = update_zed(&paths, &config);
            }
            Ok(EXIT_OK)
        }
        super::ProjectAction::Remove { path } => {
            let mut config = crate::config::load(&paths.config_file())?;
            let canonical = std::fs::canonicalize(&path).unwrap_or(path);
            config.projects.retain(|p| p != &canonical);
            crate::config::save(&paths.config_file(), &config)?;
            let _ = update_zed(&paths, &config);
            Ok(EXIT_OK)
        }
        super::ProjectAction::List => {
            let config = crate::config::load(&paths.config_file())?;
            for p in &config.projects {
                crate::ui::out(&p.to_string_lossy());
            }
            Ok(EXIT_OK)
        }
    }
}

pub fn update_zed(paths: &Paths, config: &crate::config::Config) -> anyhow::Result<()> {
    let zed_settings = match crate::paths::zed_settings_file() {
        Ok(p) => p,
        Err(_) => return Ok(()),
    };
    if !zed_settings.exists() {
        return Ok(());
    }
    let mut active_agents = Vec::new();
    for agent_spec in &crate::agents::AGENTS {
        if crate::release::active_lock(paths, agent_spec.id).is_ok() {
            active_agents.push(*agent_spec);
        }
    }
    let installed_bin = crate::paths::installed_binary()?;
    let entries = crate::zed_settings::entries_for(
        &config.projects,
        &active_agents,
        &installed_bin.to_string_lossy(),
    );
    let _ = crate::zed_settings::install(&zed_settings, &paths.backups_dir(), &entries);
    Ok(())
}

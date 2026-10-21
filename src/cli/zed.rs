//! `zevbox zed` (§8.1).

use crate::paths::Paths;
use crate::supervisor::{EXIT_ERROR, EXIT_OK, EXIT_PRECONDITION};

pub async fn run(action: super::ZedAction) -> anyhow::Result<u8> {
    let paths = Paths::from_env()?;
    if !paths.config_file().exists() {
        crate::ui::err(
            "zevbox is not set up. Fix: run 'zevbox setup' or 'zevbox init' in a macOS terminal.",
        );
        return Ok(EXIT_PRECONDITION);
    }
    // Validates the configuration before touching Zed's settings.
    let _config = crate::config::load(&paths.config_file())?;

    match action {
        super::ZedAction::Install { settings, dry_run } => {
            let settings_path = match settings {
                Some(p) => p,
                None => crate::paths::zed_settings_file()?,
            };
            let mut active_agents = Vec::new();
            for agent_spec in &crate::agents::AGENTS {
                if crate::release::active_lock(&paths, agent_spec.id).is_ok() {
                    active_agents.push(*agent_spec);
                }
            }
            let installed_bin = crate::paths::installed_binary()?;
            let entries =
                crate::zed_settings::entries_for(&active_agents, &installed_bin.to_string_lossy());
            if dry_run {
                let original = match std::fs::read_to_string(&settings_path) {
                    Ok(t) => t,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
                    Err(e) => {
                        crate::ui::err(&format!("zevbox: cannot read settings: {e}"));
                        return Ok(EXIT_ERROR);
                    }
                };
                match crate::zed_settings::apply_entries(&original, &entries) {
                    Err(e) => {
                        crate::ui::err(&format!("zevbox: {e}"));
                        return Ok(EXIT_ERROR);
                    }
                    Ok(updated) if updated == original => {
                        crate::ui::out(
                            "zed settings: already up to date (dry run, nothing written)",
                        );
                    }
                    Ok(_) => {
                        crate::ui::out(&format!(
                            "zed settings: would write {} managed entr{} in {} (dry run, nothing written):",
                            entries.len(),
                            if entries.len() == 1 { "y" } else { "ies" },
                            settings_path.display()
                        ));
                        for e in &entries {
                            crate::ui::out(&format!(
                                "  {} -> {} {}",
                                e.name,
                                e.command,
                                e.args.join(" ")
                            ));
                        }
                    }
                }
                return Ok(EXIT_OK);
            }
            match crate::zed_settings::install(&settings_path, &paths.backups_dir(), &entries) {
                Ok(outcome) => {
                    report_install(&outcome);
                    Ok(EXIT_OK)
                }
                Err(e) => {
                    crate::ui::err(&format!("zevbox: {e}"));
                    Ok(EXIT_ERROR)
                }
            }
        }
        super::ZedAction::Uninstall { settings } => {
            let settings_path = match settings {
                Some(p) => p,
                None => crate::paths::zed_settings_file()?,
            };
            match crate::zed_settings::install(&settings_path, &paths.backups_dir(), &[]) {
                Ok(outcome) => {
                    report_install(&outcome);
                    Ok(EXIT_OK)
                }
                Err(e) => {
                    crate::ui::err(&format!("zevbox: {e}"));
                    Ok(EXIT_ERROR)
                }
            }
        }
    }
}

/// What `zed install` / `uninstall` did, in one line (plan §8.1).
fn report_install(outcome: &crate::zed_settings::InstallOutcome) {
    match outcome {
        crate::zed_settings::InstallOutcome::Unchanged => {
            crate::ui::out("zed settings: already up to date");
        }
        crate::zed_settings::InstallOutcome::Updated { backup } => {
            crate::ui::out(&format!(
                "zed settings: updated (backup: {})",
                backup.display()
            ));
        }
    }
}

/// Keeps the managed Zed entries in sync (one per agent with an active
/// release). No-op write-wise unless an entry is missing or stale.
pub fn update_zed(paths: &Paths, _config: &crate::config::Config) -> anyhow::Result<()> {
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
    let entries =
        crate::zed_settings::entries_for(&active_agents, &installed_bin.to_string_lossy());
    let _ = crate::zed_settings::install(&zed_settings, &paths.backups_dir(), &entries);
    Ok(())
}

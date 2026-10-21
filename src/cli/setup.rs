//! `zevbox setup` (§8.1).

use crate::paths::Paths;
use crate::supervisor::{EXIT_ERROR, EXIT_OK};

pub async fn run() -> anyhow::Result<u8> {
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

    let config = crate::config::load(&paths.config_file())?;

    // Releases first: Zed gets one entry per agent that has an active release.
    let missing_release = crate::agents::AGENTS
        .iter()
        .any(|a| crate::release::active_lock(&paths, a.id).is_err());
    if missing_release {
        crate::ui::out("building agent releases (network, 1-3 min per agent)...");
        // §8.1: one agent failing to build does not block the other; the summary
        // below and the exit code say what is missing.
        let _ = crate::cli::update::run_update(false, None).await?;
    } else {
        crate::ui::out("releases: up to date");
    }

    authenticate_missing(&paths).await?;

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
    for spec in crate::agents::AGENTS {
        let (release, authenticated) = agent_state(&paths, spec.id);
        crate::ui::out(&format!(
            "  {}: release {release}, {}",
            spec.display,
            if authenticated {
                "authenticated"
            } else {
                "not authenticated"
            }
        ));
    }
    let incomplete = crate::agents::AGENTS
        .iter()
        .any(|a| crate::release::active_lock(&paths, a.id).is_err());
    if incomplete {
        crate::ui::err(
            "zevbox: setup incomplete: an agent has no release. Fix: run 'zevbox update' (network needed) and read the errors above.",
        );
        return Ok(EXIT_ERROR);
    }
    crate::ui::out("next: open a project in Zed, then choose a zevbox agent with '+'");
    Ok(EXIT_OK)
}

/// (active release id or "none", auth marker present).
fn agent_state(paths: &Paths, agent: &str) -> (String, bool) {
    match crate::release::active_lock(paths, agent) {
        Ok(lock) => {
            let authenticated = paths.state_dir(agent).join(&lock.auth_marker).exists();
            (lock.release_id, authenticated)
        }
        Err(_) => ("none".to_string(), false),
    }
}

/// Offers `zevbox auth` for every agent that has a release but no auth marker.
/// Only asks on a terminal; otherwise prints the command to run.
async fn authenticate_missing(paths: &Paths) -> anyhow::Result<()> {
    use std::io::IsTerminal;
    for (spec, arg) in crate::agents::AGENTS
        .iter()
        .zip([super::AgentArg::Claude, super::AgentArg::Antigravity])
    {
        let (release, authenticated) = agent_state(paths, spec.id);
        if release == "none" || authenticated {
            continue;
        }
        if !std::io::stdin().is_terminal() {
            crate::ui::out(&format!(
                "skipping authentication (not a terminal). Fix: run 'zevbox auth {}'",
                spec.id
            ));
            continue;
        }
        crate::ui::out(&format!("authenticate {} now? [y/N] ", spec.display));
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        if answer.trim().eq_ignore_ascii_case("y") {
            crate::cli::auth::run(arg, false).await?;
        }
    }
    Ok(())
}

//! `zevbox shell` (§8.1).

use crate::paths::Paths;
use crate::supervisor::{EXIT_ERROR, EXIT_OK, EXIT_PRECONDITION};
use std::path::PathBuf;

pub async fn run(agent: super::AgentArg, project: Option<PathBuf>) -> anyhow::Result<u8> {
    let paths = Paths::from_env()?;
    if !paths.config_file().exists() {
        crate::ui::err(
            "zevbox is not set up. Fix: run 'zevbox setup' or 'zevbox init' in a macOS terminal.",
        );
        return Ok(EXIT_PRECONDITION);
    }
    let config = crate::config::load(&paths.config_file())?;

    let lock = match crate::release::active_lock(&paths, agent.id()) {
        Ok(l) => l,
        Err(e) => {
            crate::ui::err(&format!(
                "no active release for {}. Fix: run 'zevbox update --agent {}' ({e})",
                agent.id(),
                agent.id()
            ));
            return Ok(EXIT_PRECONDITION);
        }
    };

    let target_project = match project {
        Some(p) => {
            let canon = match std::fs::canonicalize(&p) {
                Ok(c) => c,
                Err(_) => {
                    crate::ui::err(&format!("project not found: {}", p.display()));
                    return Ok(EXIT_ERROR);
                }
            };
            if !config.projects.contains(&canon) {
                crate::ui::err(&format!(
                    "{} is not an allowed project. Fix: run 'zevbox project add {}'",
                    canon.display(),
                    canon.display()
                ));
                return Ok(EXIT_ERROR);
            }
            canon
        }
        None => {
            let cur = std::env::current_dir()
                .ok()
                .and_then(|c| std::fs::canonicalize(c).ok());
            match cur.filter(|c| config.projects.contains(c)) {
                Some(c) => c,
                None => {
                    let shown = std::env::current_dir()
                        .map(|c| c.display().to_string())
                        .unwrap_or_else(|_| "the current directory".to_string());
                    crate::ui::err(&format!(
                        "{shown} is not an allowed project. Fix: run 'zevbox project add {shown}' or pass --project <path>"
                    ));
                    return Ok(EXIT_PRECONDITION);
                }
            }
        }
    };

    crate::ui::out(&format!(
        "zevbox Linux shell — agent={} project={} release={} (changes outside the project are discarded on exit)",
        agent.id(),
        target_project.display(),
        lock.release_id
    ));

    crate::fsutil::ensure_private_dir(&paths.state_dir(agent.id()))?;

    let id8 = crate::container::new_instance_id();
    let name = format!("zevbox-{}-sh-{id8}", agent.id());

    let spec = crate::container::RunSpec {
        name,
        instance_id: format!("sh-{id8}"),
        agent: agent.id().to_string(),
        cpus: config.resources.cpus,
        memory: config.resources.memory.clone(),
        uid: config.host_uid,
        gid: config.host_gid,
        project: target_project,
        state_dir: paths.state_dir(agent.id()),
        image: lock.image,
        command: vec!["/bin/bash".to_string()],
        interactive: true,
    };

    let args = crate::container::run_args(&spec);
    let mut cmd = std::process::Command::new(&config.container_binary);
    cmd.args(&args);
    cmd.stdin(std::process::Stdio::inherit());
    cmd.stdout(std::process::Stdio::inherit());
    cmd.stderr(std::process::Stdio::inherit());

    let status = cmd.status()?;
    if status.success() {
        Ok(EXIT_OK)
    } else {
        Ok(EXIT_ERROR)
    }
}

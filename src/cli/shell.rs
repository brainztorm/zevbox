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

    // Same zone rules as `launch` (I2, DECISIONS E1).
    let rules = crate::paths::project_rules(&paths)?;
    let explicit = project.is_some();
    let candidate = match project {
        Some(p) => p,
        None => std::env::current_dir()?,
    };
    let target_project = match crate::config::validate_project_path(&candidate, &rules) {
        Ok(c) => c,
        Err(e) => {
            let reason = match e {
                crate::config::ConfigError::ProjectRejected { reason, .. } => reason,
                other => other.to_string(),
            };
            crate::ui::err(&format!(
                "{} is not an allowed project: {reason}. {}",
                candidate.display(),
                crate::config::PROJECT_ZONES_FIX
            ));
            return Ok(if explicit {
                EXIT_ERROR
            } else {
                EXIT_PRECONDITION
            });
        }
    };

    crate::ui::out(&format!(
        "zevbox Linux shell — agent={} project={} release={} (changes outside the project are discarded on exit)",
        agent.id(),
        target_project.display(),
        lock.release_id
    ));

    // Same project-file protections as the agent's VM (I13): `.zed` masked, an existing
    // `.git` read-only. Same precondition: `.zed` must not be a file or a link.
    if let Some(problem) = crate::guard::project_files_problem(&target_project) {
        crate::ui::err(&format!("zevbox: {problem}"));
        return Ok(EXIT_PRECONDITION);
    }
    let git_exists = std::fs::symlink_metadata(target_project.join(".git")).is_ok();
    crate::ui::out("(.zed is masked and .git is read-only in this shell, as for the agent)");

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

    let args = crate::container::launch_args(&spec, git_exists);
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

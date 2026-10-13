//! `zevbox auth` (§8.1).

use crate::paths::Paths;
use crate::supervisor::{EXIT_ERROR, EXIT_OK, EXIT_PRECONDITION};

pub async fn run(agent: super::AgentArg, status: bool) -> anyhow::Result<u8> {
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
        Err(_) => {
            if status {
                crate::ui::out("unknown");
                return Ok(EXIT_OK);
            } else {
                crate::ui::err(&format!(
                    "no active release for {}. Fix: run 'zevbox update --agent {}'",
                    agent.id(),
                    agent.id()
                ));
                return Ok(EXIT_PRECONDITION);
            }
        }
    };

    let marker_path = paths.state_dir(agent.id()).join(&lock.auth_marker);

    if status {
        if marker_path.exists() {
            crate::ui::out("authenticated");
        } else {
            crate::ui::out("not authenticated");
        }
        return Ok(EXIT_OK);
    }

    // Ensure state dir exists with 0700
    crate::fsutil::ensure_private_dir(&paths.state_dir(agent.id()))?;

    let project = match config.projects.first() {
        Some(p) => p.clone(),
        None => match std::env::current_dir() {
            Ok(p) => p,
            Err(_) => std::path::PathBuf::from("/"),
        },
    };

    let id8 = crate::container::new_instance_id();
    let name = format!("zevbox-{}-sh-{id8}", agent.id());

    let spec = crate::container::RunSpec {
        name,
        instance_id: format!("auth-{id8}"),
        agent: agent.id().to_string(),
        cpus: config.resources.cpus,
        memory: config.resources.memory.clone(),
        uid: config.host_uid,
        gid: config.host_gid,
        project,
        state_dir: paths.state_dir(agent.id()),
        image: lock.image,
        command: lock.auth_command,
        interactive: true,
    };

    let args = crate::container::run_args(&spec);
    let mut cmd = std::process::Command::new(&config.container_binary);
    cmd.args(&args);
    cmd.stdin(std::process::Stdio::inherit());
    cmd.stdout(std::process::Stdio::inherit());
    cmd.stderr(std::process::Stdio::inherit());

    let exit_status = cmd.status()?;
    if exit_status.success() {
        Ok(EXIT_OK)
    } else {
        Ok(EXIT_ERROR)
    }
}

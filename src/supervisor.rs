//! `zevbox launch`: preflight, container start, relay, bounded shutdown (§8.3–§8.5).

use crate::paths::Paths;
use std::path::Path;
use std::time::Duration;

/// Exit codes of the whole CLI (§8.1).
pub const EXIT_OK: u8 = 0;
pub const EXIT_ERROR: u8 = 1;
pub const EXIT_USAGE: u8 = 2;
pub const EXIT_PRECONDITION: u8 = 3;

struct PreflightOk {
    config: crate::config::Config,
    lock: crate::release::ReleaseLock,
    canonical_project: std::path::PathBuf,
    cli: crate::container::ContainerCli,
}

async fn check_preflight(
    paths: &Paths,
    agent: &str,
    project: &Path,
) -> Result<PreflightOk, String> {
    // 1. Config present and readable
    let config = match crate::config::load(&paths.config_file()) {
        Ok(c) => c,
        Err(_) => {
            return Err(
                "zevbox is not set up. Fix: run 'zevbox setup' in a macOS terminal.".to_string(),
            );
        }
    };

    // 2. Project allowed (I2, zones of DECISIONS E1). Choosing a zevbox agent
    // in Zed is the user's consent (D1): no allow-list, the zone rules decide.
    let rules = crate::paths::project_rules(paths).map_err(|e| format!("{e}"))?;
    let canonical_project = match crate::config::validate_project_path(project, &rules) {
        Ok(p) => p,
        Err(crate::config::ConfigError::ProjectRejected { reason, .. }) => {
            return Err(format!(
                "{} is not an allowed project: {reason}. {}",
                project.display(),
                crate::config::PROJECT_ZONES_FIX
            ));
        }
        Err(e) => return Err(e.to_string()),
    };

    // 3. Known agent
    let agent_spec = match crate::agents::find(agent) {
        Some(a) => a,
        None => {
            return Err(format!("unknown agent '{agent}'."));
        }
    };

    // 4. Active release & image present
    let lock = match crate::release::active_lock(paths, agent) {
        Ok(l) => l,
        Err(crate::release::ReleaseError::NoActiveRelease(_)) => {
            return Err(format!(
                "no active {} release. Fix: run 'zevbox update --agent {agent}' in a macOS terminal.",
                agent_spec.display
            ));
        }
        Err(e) => {
            return Err(format!("error reading release lock: {e}"));
        }
    };

    let cli = crate::container::ContainerCli::new(config.container_binary.clone());
    let images = match cli.image_list().await {
        Ok(imgs) => imgs,
        Err(e) => {
            return Err(format!("cannot list container images: {e}"));
        }
    };
    if !images
        .iter()
        .any(|img| crate::container::image_matches(&img.reference, &lock.image))
    {
        return Err(format!(
            "no active {} image '{}'. Fix: run 'zevbox update --agent {agent}' in a macOS terminal.",
            agent_spec.display, lock.image
        ));
    }

    // 5. Runtime running
    let status = match cli.system_status().await {
        Ok(s) => s,
        Err(e) => {
            return Err(format!("cannot query Apple Container system status: {e}"));
        }
    };
    if !status.running {
        let _ = cli.system_start().await;
        let status_after = match cli.system_status().await {
            Ok(s) => s,
            Err(e) => {
                return Err(format!("cannot query Apple Container system status: {e}"));
            }
        };
        if !status_after.running {
            return Err(
                "Apple Container system is not running. Fix: run 'container system start'."
                    .to_string(),
            );
        }
    }

    // 6. Auth marker
    let marker_path = paths.state_dir(agent).join(&lock.auth_marker);
    if !marker_path.exists() {
        return Err(format!(
            "{} is not authenticated. Fix: run 'zevbox auth {agent}' in a macOS terminal.",
            agent_spec.display
        ));
    }

    Ok(PreflightOk {
        config,
        lock,
        canonical_project,
        cli,
    })
}

/// Full `launch` flow. Uses the process stdin/stdout for ACP. Returns the exit code.
/// On preflight failure: answer Zed's first request with the §8.5 error, exit 3.
pub async fn launch(paths: &Paths, agent: &str, project: &Path) -> u8 {
    let preflight = match check_preflight(paths, agent, project).await {
        Ok(p) => p,
        Err(err_msg) => {
            let full_msg = format!("zevbox: {err_msg}");
            use std::io::{BufRead, Write};
            let stdin = std::io::stdin();
            let mut handle = stdin.lock();
            let mut line = String::new();
            while handle.read_line(&mut line).is_ok() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    line.clear();
                    continue;
                }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    if let Some(id) = v.get("id") {
                        let resp = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {
                                "code": -32000,
                                "message": full_msg
                            }
                        });
                        let mut out = std::io::stdout();
                        let _ = out.write_all(format!("{resp}\n").as_bytes());
                        let _ = out.flush();
                        break;
                    }
                }
                break;
            }
            crate::ui::err(&full_msg);
            return EXIT_PRECONDITION;
        }
    };

    let PreflightOk {
        config,
        lock,
        canonical_project,
        cli,
    } = preflight;

    // 2. Clean orphans silently (§8.4)
    clean_orphans(paths).await;

    // 3. Prepare instance and container arguments
    let id = crate::container::new_instance_id();
    let container_name = format!("zevbox-{agent}-{id}");
    let spec = crate::container::RunSpec {
        name: container_name.clone(),
        instance_id: id.clone(),
        agent: agent.to_string(),
        cpus: config.resources.cpus,
        memory: config.resources.memory.clone(),
        uid: config.host_uid,
        gid: config.host_gid,
        project: canonical_project.clone(),
        state_dir: paths.state_dir(agent),
        image: lock.image.clone(),
        command: lock.agent_command.clone(),
        interactive: false,
    };
    // EXP-01 / P1: mask .zed, lock an existing .git, guard the rest (guard.rs).
    let root_git_existed = std::fs::symlink_metadata(canonical_project.join(".git")).is_ok();
    let run_args = crate::container::launch_args(&spec, root_git_existed);

    let _ = crate::fsutil::ensure_private_dir(&paths.logs_dir());
    let log_file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.log_file(&id))
    {
        Ok(f) => f,
        Err(e) => {
            crate::ui::err(&format!("zevbox: cannot open log file: {e}"));
            return EXIT_ERROR;
        }
    };

    let mut cmd = tokio::process::Command::new(&config.container_binary);
    cmd.args(&run_args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::from(log_file))
        .kill_on_drop(true);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            crate::ui::err(&format!("zevbox: failed to spawn container: {e}"));
            return EXIT_ERROR;
        }
    };

    let relay_pid = std::process::id();
    let relay_started_at = crate::instances::process_start_time(relay_pid).unwrap_or_default();
    let record = crate::instances::InstanceRecord {
        id: id.clone(),
        agent: agent.to_string(),
        project: canonical_project.clone(),
        release: lock.release_id.clone(),
        image: lock.image.clone(),
        container_name: container_name.clone(),
        relay_pid,
        relay_started_at,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    if let Err(e) = crate::instances::write(paths, &record) {
        tracing::warn!("failed to write instance record: {e}");
    }

    let shutdown = tokio_util::sync::CancellationToken::new();
    let sig_token = shutdown.clone();
    // Fail closed: without the guard, a new .git in the project goes unnoticed.
    let guard = match crate::guard::start(&canonical_project, root_git_existed, shutdown.clone()) {
        Ok(g) => g,
        Err(e) => {
            crate::ui::err(&format!("zevbox: cannot watch the project folder: {e}"));
            let _ = child.start_kill();
            let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
            let _ = cli.stop_and_delete_verified(&container_name).await;
            let _ = crate::instances::remove(paths, &id);
            return EXIT_ERROR;
        }
    };
    let mut sigterm =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
    let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()).ok();
    let mut sighup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup()).ok();

    tokio::spawn(async move {
        tokio::select! {
            _ = async {
                match sigterm.as_mut() {
                    Some(s) => { s.recv().await; }
                    None => std::future::pending::<()>().await,
                }
            } => {
                sig_token.cancel();
            }
            _ = async {
                match sigint.as_mut() {
                    Some(s) => { s.recv().await; }
                    None => std::future::pending::<()>().await,
                }
            } => {
                sig_token.cancel();
            }
            _ = async {
                match sighup.as_mut() {
                    Some(s) => { s.recv().await; }
                    None => std::future::pending::<()>().await,
                }
            } => {
                sig_token.cancel();
            }
        }
    });

    let Some(agent_out) = child.stdout.take() else {
        return EXIT_ERROR;
    };
    let Some(agent_in) = child.stdin.take() else {
        return EXIT_ERROR;
    };
    let client_in = tokio::io::stdin();
    let client_out = tokio::io::stdout();

    let policy = crate::relay::policy::Policy::new(&canonical_project.to_string_lossy(), agent);
    let relay_end = crate::relay::run_relay(
        client_in,
        client_out,
        agent_out,
        agent_in,
        policy,
        config.relay.max_message_bytes,
        shutdown,
    )
    .await;

    // Bounded shutdown (§8.3)
    // 1. Wait up to 3s for container to stop
    let exited = matches!(
        tokio::time::timeout(Duration::from_secs(3), child.wait()).await,
        Ok(Ok(_))
    );

    // 2. Stop container if still running
    if !exited {
        let _ = cli.stop(&container_name, 3).await;
        let stopped = matches!(
            tokio::time::timeout(Duration::from_secs(5), child.wait()).await,
            Ok(Ok(_))
        );
        // 3. Kill process if still not exited
        if !stopped {
            let _ = child.start_kill();
            let _ = tokio::time::timeout(Duration::from_secs(1), child.wait()).await;
        }
    }

    // 4. Check if container still exists and delete --force
    if let Ok(containers) = cli.list_all().await {
        if let Some(c) = containers.into_iter().find(|c| c.name == container_name) {
            if crate::container::may_target(&c) {
                let _ = cli.delete_force(&container_name).await;
            }
        }
    }

    // 5. Verify container is gone and clean up instance record
    let gone = match cli.list_all().await {
        Ok(list) => !list.iter().any(|c| c.name == container_name),
        Err(_) => true,
    };

    if gone {
        let _ = crate::instances::remove(paths, &id);
    }

    if let Some(path) = guard.tripped() {
        crate::ui::err(&crate::guard::tripped_message(&path));
        return EXIT_ERROR;
    }
    drop(guard);

    let is_error = matches!(relay_end, crate::relay::RelayEnd::Fatal(_)) || !gone;
    if is_error {
        EXIT_ERROR
    } else {
        EXIT_OK
    }
}

/// Removes orphan instances and their verified containers (§8.4).
/// Returns the ids cleaned. Never fails the caller: errors are logged.
pub async fn clean_orphans(paths: &Paths) -> Vec<String> {
    let mut cleaned = Vec::new();
    let records = match crate::instances::list(paths) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("failed to list instances: {e}");
            return cleaned;
        }
    };
    let config = match crate::config::load(&paths.config_file()) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("failed to load config during clean_orphans: {e}");
            return cleaned;
        }
    };
    let cli = crate::container::ContainerCli::new(config.container_binary.clone());
    for rec in records {
        if crate::instances::is_orphan(&rec) {
            tracing::info!(id = %rec.id, name = %rec.container_name, "cleaning orphan instance");
            let _ = cli.stop_and_delete_verified(&rec.container_name).await;
            let _ = crate::instances::remove(paths, &rec.id);
            cleaned.push(rec.id);
        }
    }
    cleaned
}

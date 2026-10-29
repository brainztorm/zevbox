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

    // I13 preconditions: a `.zed` that is a file or a link cannot be masked safely.
    if let Some(problem) = crate::guard::project_files_problem(&canonical_project) {
        return Err(problem);
    }

    // 3. Known agent
    let agent_spec = match crate::agents::find(agent) {
        Some(a) => a,
        None => {
            return Err(format!("unknown agent '{agent}'."));
        }
    };

    // 4. Active release
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
    // 4. Runtime running, started if needed. Before anything that talks to the
    // runtime: `image list` fails with an XPC error while it is stopped (e.g. Zed
    // reopening a thread right after a reboot).
    // `container system status` exits 1 with {"status":"unregistered"} while the
    // service is stopped, so an error here means "not running", not "unknown".
    let running = matches!(cli.system_status().await, Ok(s) if s.running);
    if !running {
        let start = cli.system_start().await;
        if !matches!(cli.system_status().await, Ok(s) if s.running) {
            return Err(runtime_down_message(start.err().map(|e| e.to_string())));
        }
    }

    // 5. Image present
    let images = match cli.image_list().await {
        Ok(imgs) => imgs,
        // The runtime was up a moment ago: same advice, with the technical reason.
        Err(e) => return Err(runtime_down_message(Some(e.to_string()))),
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

    // 6. Auth marker: the same test as `auth --status` and `doctor` (Antigravity:
    // a non-empty refresh token; oMLX: a server and a key), DECISIONS backlog 4.
    if !crate::cli::auth::is_authenticated(paths, agent, &lock.auth_marker) {
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

/// Sends `tracing` events to the instance's `logs/<id>.log` (plan §4 step 5, I6: never
/// stdout). Level `info`, or `RUST_LOG`. I9: events carry names, paths, ids and sizes,
/// never message content. Best effort: a missing subscriber must not stop a launch.
fn init_launch_logging(log_file: &std::fs::File) {
    let Ok(file) = log_file.try_clone() else {
        return;
    };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::sync::Mutex::new(file))
        .with_ansi(false)
        .with_target(false)
        .try_init();
}

/// Name of how the relay ended, without any payload (I9).
fn relay_end_name(end: &crate::relay::RelayEnd) -> &'static str {
    match end {
        crate::relay::RelayEnd::ClientEof => "client_eof",
        crate::relay::RelayEnd::AgentEof => "agent_eof",
        crate::relay::RelayEnd::Shutdown => "shutdown",
        crate::relay::RelayEnd::Fatal(_) => "fatal",
    }
}

/// What Zed shows when the container service is down and zevbox could not
/// start it (e.g. right after a reboot). Plain words first, the reason last.
fn runtime_down_message(reason: Option<String>) -> String {
    let mut msg = "Apple Container system is not running, and zevbox could not start it. \
         Fix: open a macOS terminal, run 'container system start', then start a new thread in Zed."
        .to_string();
    if let Some(r) = reason {
        msg.push_str(&format!(" (technical detail: {r})"));
    }
    msg
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
            // Ok(0) is end of input: stop, or this loop would spin forever.
            while matches!(handle.read_line(&mut line), Ok(n) if n > 0) {
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

    // I13: watch the project BEFORE the container starts, so nothing the agent does
    // in its first moments can slip past the watcher. Fail closed.
    let shutdown = tokio_util::sync::CancellationToken::new();
    let sig_token = shutdown.clone();
    let guard = match crate::guard::start(&canonical_project, root_git_existed, shutdown.clone()) {
        Ok(g) => g,
        Err(e) => {
            crate::ui::err(&format!("zevbox: cannot watch the project folder: {e}"));
            return EXIT_ERROR;
        }
    };

    init_launch_logging(&log_file);
    tracing::info!(
        "launch agent={agent} release={} container={container_name} project={}",
        lock.release_id,
        canonical_project.display()
    );

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

    tracing::info!(
        "shutdown relay_end={} container_gone={gone}",
        relay_end_name(&relay_end)
    );
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

#[cfg(test)]
mod tests {
    use super::runtime_down_message;

    #[test]
    fn runtime_message_is_actionable_and_keeps_the_reason_last() {
        let m = runtime_down_message(Some("XPC connection error".into()));
        assert!(
            m.starts_with("Apple Container system is not running"),
            "{m}"
        );
        assert!(m.contains("container system start"), "{m}");
        assert!(m.contains("new thread in Zed"), "{m}");
        assert!(
            m.ends_with("(technical detail: XPC connection error)"),
            "{m}"
        );
        assert!(!runtime_down_message(None).contains("technical detail"));
    }
}

//! `zevbox doctor` (§8.1, §8.6).

use crate::paths::Paths;
use crate::supervisor::{EXIT_ERROR, EXIT_OK, EXIT_PRECONDITION};
use serde_json::json;

pub async fn run(json_output: bool) -> anyhow::Result<u8> {
    let paths = Paths::from_env()?;
    if !paths.config_file().exists() {
        crate::ui::err(
            "zevbox is not set up. Fix: run 'zevbox setup' or 'zevbox init' in a macOS terminal.",
        );
        return Ok(EXIT_PRECONDITION);
    }

    let mut checks: Vec<(String, String, String)> = Vec::new(); // (name, status, message)

    let config = match crate::config::load(&paths.config_file()) {
        Ok(c) => {
            checks.push((
                "config".to_string(),
                "OK".to_string(),
                "config.json is valid".to_string(),
            ));
            c
        }
        Err(e) => {
            checks.push((
                "config".to_string(),
                "FAIL".to_string(),
                format!("config.json invalid: {e}"),
            ));
            return finish(json_output, &checks);
        }
    };

    // 1. containerBinary executable and version >= 1.5.0
    let cli = crate::container::ContainerCli::new(config.container_binary.clone());
    match cli.system_status().await {
        Ok(sys) => {
            let (status, message) = match crate::container::version_at_least(
                &sys.version,
                crate::container::MIN_CONTAINER_VERSION,
            ) {
                Some(true) => (
                    "OK",
                    format!("Apple Container {} ({})", sys.version, sys.architecture),
                ),
                Some(false) => (
                    "FAIL",
                    format!(
                        "Apple Container {} is older than the minimum 1.5.0. Fix: upgrade it (brew upgrade container)",
                        sys.version
                    ),
                ),
                None => (
                    "WARN",
                    format!("cannot read the Apple Container version ('{}')", sys.version),
                ),
            };
            checks.push(("container_binary".to_string(), status.to_string(), message));

            // 2. system status running
            if sys.running {
                checks.push((
                    "system_status".to_string(),
                    "OK".to_string(),
                    "Apple Container runtime is running".to_string(),
                ));
            } else {
                checks.push((
                    "system_status".to_string(),
                    "WARN".to_string(),
                    "runtime not running (start with 'container system start')".to_string(),
                ));
            }

            // 3. Architecture arm64
            if sys.architecture == "arm64" || sys.architecture == "aarch64" {
                checks.push((
                    "architecture".to_string(),
                    "OK".to_string(),
                    "host architecture is arm64".to_string(),
                ));
            } else {
                checks.push((
                    "architecture".to_string(),
                    "WARN".to_string(),
                    format!("architecture is {}", sys.architecture),
                ));
            }
        }
        Err(e) => {
            checks.push((
                "container_binary".to_string(),
                "FAIL".to_string(),
                format!("cannot query container CLI: {e}"),
            ));
        }
    }

    // 5. Active agents: release, image present, authentication (plan §8.6 point 5)
    let images = cli.image_list().await.ok(); // None while the runtime is down
    let active = crate::release::load_active(&paths).unwrap_or_default();
    for spec in crate::agents::AGENTS {
        let name = format!("agent:{}", spec.id);
        let Some(entry) = active.get(spec.id) else {
            if spec.opt_in {
                checks.push((
                    name,
                    "OK".to_string(),
                    format!(
                        "optional, not installed (to use it: 'zevbox auth {}')",
                        spec.id
                    ),
                ));
                continue;
            }
            checks.push((
                name,
                "WARN".to_string(),
                "no active release installed".to_string(),
            ));
            continue;
        };
        let lock = match crate::release::load_lock(&paths, &entry.current) {
            Ok(l) => l,
            Err(e) => {
                checks.push((
                    name,
                    "FAIL".to_string(),
                    format!("corrupt lock for {}: {e}", entry.current),
                ));
                continue;
            }
        };
        let image_present = images.as_ref().map(|list| {
            list.iter()
                .any(|i| crate::container::image_matches(&i.reference, &lock.image))
        });
        let authenticated = crate::cli::auth::is_authenticated(&paths, spec.id, &lock.auth_marker);
        let (status, message) = agent_check(
            spec.id,
            &entry.current,
            image_present,
            authenticated,
            spec.id == "claude" && !has_default_model(&paths),
        );
        checks.push((name, status.to_string(), message));
        if spec.id == "omlx" && authenticated && image_present == Some(true) {
            checks.push(omlx_reach_check(&paths, &cli, &lock.image).await);
        }
    }

    // 6. Disk used by zevbox images (DECISIONS G1)
    if let Ok(images) = cli.image_list().await {
        let zevbox = images
            .iter()
            .filter(|i| crate::container::is_zevbox_image(&i.reference))
            .count();
        let stale = crate::release::stale_images_here(&paths, &images).len();
        let (status, message) = disk_message(zevbox, stale, cli.images_bytes().await);
        checks.push(("disk".to_string(), status.to_string(), message));
    }

    // 7. Permissions 0700 on stateRoot and state/<agent> (plan §8.6 point 6)
    checks.push(permissions_check(&paths));

    // 8. Installed binary and Zed entries (plan §8.6 points 7 and 8)
    checks.extend(zed_checks(&paths));

    // 9. Orphan or unregistered instances (plan §8.6 point 9)
    if let Ok(containers) = cli.list_ours(&paths.state_root).await {
        checks.push(instances_check(&paths, &containers));
    }

    // 10. ACP methods refused recently, read from the instance logs (plan §7.3, §8.6)
    checks.push(blocked_methods_check(&paths));

    finish(json_output, &checks)
}

/// Counts `blocked method=<m> dir=<d>` lines (written by the relay, I4) per method.
/// Only the logs of the last days count: an old refusal must not be reported until
/// the log is pruned (DECISIONS backlog 6).
const BLOCKED_METHODS_DAYS: u64 = 7;

fn count_blocked_methods(logs: &[String]) -> std::collections::BTreeMap<String, usize> {
    let mut counts = std::collections::BTreeMap::new();
    for text in logs {
        for line in text.lines() {
            let Some(rest) = line.split("blocked method=").nth(1) else {
                continue;
            };
            let method = rest.split_whitespace().next().unwrap_or("-");
            if crate::relay::policy::AUDITED_REFUSALS.contains(&method) {
                continue;
            }
            *counts.entry(method.to_string()).or_insert(0) += 1;
        }
    }
    counts
}

fn blocked_methods_check(paths: &Paths) -> (String, String, String) {
    let window = std::time::Duration::from_secs(BLOCKED_METHODS_DAYS * 24 * 3600);
    let recent = |e: &std::fs::DirEntry| {
        e.metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_none_or(|age| age <= window)
    };
    let logs: Vec<String> = std::fs::read_dir(paths.logs_dir())
        .map(|dir| {
            dir.filter_map(Result::ok)
                .filter(|e| e.path().extension().is_some_and(|x| x == "log"))
                .filter(recent)
                .filter_map(|e| std::fs::read_to_string(e.path()).ok())
                .collect()
        })
        .unwrap_or_default();
    let counts = count_blocked_methods(&logs);
    if counts.is_empty() {
        return (
            "blocked_methods".to_string(),
            "OK".to_string(),
            format!("no ACP method refused in the last {BLOCKED_METHODS_DAYS} days"),
        );
    }
    let list = counts
        .iter()
        .map(|(m, n)| format!("{m} ({n}x)"))
        .collect::<Vec<_>>()
        .join(", ");
    (
        "blocked_methods".to_string(),
        "WARN".to_string(),
        format!("refused in the last {BLOCKED_METHODS_DAYS} days: {list}. If one looks legitimate, audit it before allowing it (AGENTS.md, plan §7.3); see 'zevbox logs'"),
    )
}

/// Pure text of the disk check: WARN when old release images can be reclaimed.
fn disk_message(zevbox: usize, stale: usize, images_bytes: Option<u64>) -> (&'static str, String) {
    let total = images_bytes
        .map(|b| format!("; all container images use {:.1} GB", b as f64 / 1e9))
        .unwrap_or_default();
    if stale == 0 {
        (
            "OK",
            format!("{zevbox} zevbox image(s), none from old releases{total}"),
        )
    } else {
        (
            "WARN",
            format!(
                "{zevbox} zevbox image(s), {stale} from old releases{total}. Fix: run 'zevbox clean --images'"
            ),
        )
    }
}

/// Pure text of one agent line. `image_present` is `None` when the runtime could not be
/// asked. FAIL when launch would fail (no image); WARN when the user must act (sign in,
/// choose a default model).
fn agent_check(
    agent: &str,
    release: &str,
    image_present: Option<bool>,
    authenticated: bool,
    missing_model: bool,
) -> (&'static str, String) {
    let auth = if authenticated {
        "authenticated"
    } else {
        "not authenticated"
    };
    let head = format!("release {release} ({auth})");
    if image_present == Some(false) {
        return (
            "FAIL",
            format!("{head}, but its image is missing. Fix: run 'zevbox update --agent {agent}'"),
        );
    }
    if !authenticated {
        return (
            "WARN",
            format!("{head}. Fix: run 'zevbox auth {agent}' in a macOS terminal"),
        );
    }
    if missing_model {
        return (
            "WARN",
            format!("{head}; no default model in state/claude/.claude/settings.json (Auto mode falls back to Accept edits in Zed). Fix: run 'zevbox auth claude' or write {{\"model\": \"fable\"}} in that file"),
        );
    }
    ("OK", head)
}

fn permissions_check(paths: &Paths) -> (String, String, String) {
    use std::os::unix::fs::PermissionsExt;
    let mut wrong = Vec::new();
    let mut dirs = vec![paths.state_root.clone()];
    // An opt-in agent's folder may not exist yet (installations made before it).
    dirs.extend(
        crate::agents::AGENTS
            .iter()
            .map(|a| paths.state_dir(a.id))
            .filter(|d| d.exists() || !d.ends_with("omlx")),
    );
    for dir in dirs {
        match std::fs::metadata(&dir) {
            Ok(m) => {
                let mode = m.permissions().mode() & 0o777;
                if mode != 0o700 {
                    wrong.push(format!("{} is 0o{mode:o}", dir.display()));
                }
            }
            Err(e) => wrong.push(format!("{}: {e}", dir.display())),
        }
    }
    if wrong.is_empty() {
        (
            "permissions".to_string(),
            "OK".to_string(),
            "stateRoot and state/* are 0700".to_string(),
        )
    } else {
        (
            "permissions".to_string(),
            "WARN".to_string(),
            format!(
                "{}, expected 0700. Fix: run 'zevbox init' or chmod 700",
                wrong.join("; ")
            ),
        )
    }
}

/// Installed binary exists, and the managed Zed entries are in sync with the agents
/// that have an active release (amendment 2026-10-14: no per-project entries).
fn zed_checks(paths: &Paths) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let Ok(binary) = crate::paths::installed_binary() else {
        return out;
    };
    if binary.exists() {
        out.push((
            "installed_binary".to_string(),
            "OK".to_string(),
            format!("{} exists", binary.display()),
        ));
    } else {
        out.push((
            "installed_binary".to_string(),
            "WARN".to_string(),
            format!(
                "{} is missing. Fix: run 'make install' in the zevbox source folder",
                binary.display()
            ),
        ));
    }
    let Ok(settings) = crate::paths::zed_settings_file() else {
        return out;
    };
    let Ok(original) = std::fs::read_to_string(&settings) else {
        out.push((
            "zed_entries".to_string(),
            "WARN".to_string(),
            format!(
                "{} not found. Fix: run 'zevbox zed install' (once Zed has created its settings)",
                settings.display()
            ),
        ));
        return out;
    };
    let active: Vec<_> = crate::agents::AGENTS
        .iter()
        .filter(|a| crate::release::active_lock(paths, a.id).is_ok())
        .copied()
        .collect();
    let entries = crate::zed_settings::entries_for(&active, &binary.to_string_lossy());
    match crate::zed_settings::apply_entries(&original, &entries) {
        Ok(updated) if updated == original => out.push((
            "zed_entries".to_string(),
            "OK".to_string(),
            "Zed entries match the agents with an active release".to_string(),
        )),
        Ok(_) => out.push((
            "zed_entries".to_string(),
            "WARN".to_string(),
            "Zed entries are missing or stale. Fix: run 'zevbox zed install'".to_string(),
        )),
        Err(e) => out.push((
            "zed_entries".to_string(),
            "WARN".to_string(),
            format!("cannot read Zed settings: {e}"),
        )),
    }
    out
}

fn instances_check(
    paths: &Paths,
    containers: &[crate::container::ContainerInfo],
) -> (String, String, String) {
    let records = crate::instances::list(paths).unwrap_or_default();
    let orphans = records
        .iter()
        .filter(|r| crate::instances::is_orphan(r))
        .count();
    let unregistered = containers
        .iter()
        .filter(|c| !records.iter().any(|r| r.container_name == c.name))
        .count();
    if orphans == 0 && unregistered == 0 {
        (
            "instances".to_string(),
            "OK".to_string(),
            format!("{} active instance(s), no orphan", records.len()),
        )
    } else {
        (
            "instances".to_string(),
            "WARN".to_string(),
            format!("{orphans} orphan record(s), {unregistered} unregistered container(s). Fix: run 'zevbox clean'"),
        )
    }
}

/// True iff the container's Claude settings hold a `model` string (DECISIONS R8).
/// The oMLX server as the agent VM sees it (its network only uses the Mac's
/// primary interface, DECISIONS 2026-10-29). No key is sent: 401 means reachable.
async fn omlx_reach_check(
    paths: &Paths,
    cli: &crate::container::ContainerCli,
    image: &str,
) -> (String, String, String) {
    let name = "omlx:server".to_string();
    let settings = paths.state_dir("omlx").join(crate::omlx::AUTH_MARKER);
    let Some(url) = crate::omlx::configured_url(&settings) else {
        return (
            name,
            "WARN".to_string(),
            "no server configured. Fix: run 'zevbox auth omlx'".to_string(),
        );
    };
    match cli.http_status_from_vm(image, &format!("{url}/v1/models")).await {
        Ok(code) if code != "000" => (name, "OK".to_string(), format!("{url} reachable from the agent VM (HTTP {code})")),
        Ok(_) => (
            name,
            "WARN".to_string(),
            format!("{url} is not reachable from the agent VM (VMs only use the Mac's main network, not a direct cable to the server). Fix: run 'zevbox auth omlx --url http://<server address on the main network>:<port>'"),
        ),
        Err(e) => (name, "WARN".to_string(), format!("cannot test {url} from a VM: {e}")),
    }
}

fn has_default_model(paths: &Paths) -> bool {
    let path = paths
        .state_dir("claude")
        .join(".claude")
        .join("settings.json");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .is_some_and(|v| v.get("model").is_some_and(|m| m.is_string()))
}

fn finish(json_output: bool, checks: &[(String, String, String)]) -> anyhow::Result<u8> {
    if json_output {
        let items: Vec<_> = checks
            .iter()
            .map(|(name, status, msg)| {
                json!({
                    "check": name,
                    "status": status,
                    "message": msg
                })
            })
            .collect();
        crate::ui::out(&serde_json::to_string_pretty(&items)?);
    } else {
        for (name, status, msg) in checks {
            crate::ui::out(&format!("[{status:4}] {name:20} — {msg}"));
        }
    }

    let has_fail = checks.iter().any(|(_, status, _)| status == "FAIL");
    if has_fail {
        Ok(EXIT_ERROR)
    } else {
        Ok(EXIT_OK)
    }
}

#[cfg(test)]
mod tests {
    use super::{agent_check, blocked_methods_check, count_blocked_methods, disk_message};
    use crate::paths::Paths;

    #[test]
    fn audited_refusals_are_not_reported() {
        let log = "x INFO blocked method=_auth/status_update dir=agent_to_client\n\
                   x INFO blocked method=terminal/create dir=agent_to_client\n";
        let counts = count_blocked_methods(&[log.to_string()]);
        assert_eq!(counts.len(), 1);
        assert_eq!(counts.get("terminal/create"), Some(&1));
    }

    #[test]
    fn only_recent_logs_count() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::new(tmp.path());
        std::fs::create_dir_all(paths.logs_dir()).unwrap();
        let old = paths.logs_dir().join("0a1b2c3d.log");
        std::fs::write(
            &old,
            "x INFO blocked method=fs/read_text_file dir=agent_to_client\n",
        )
        .unwrap();
        let eight_days = std::time::Duration::from_secs(8 * 24 * 3600);
        let file = std::fs::File::options().write(true).open(&old).unwrap();
        file.set_modified(std::time::SystemTime::now() - eight_days)
            .unwrap();
        assert_eq!(blocked_methods_check(&paths).1, "OK");
        std::fs::write(
            paths.logs_dir().join("deadbeef.log"),
            "x INFO blocked method=terminal/create dir=agent_to_client\n",
        )
        .unwrap();
        let (_, status, message) = blocked_methods_check(&paths);
        assert_eq!(status, "WARN");
        assert!(message.contains("terminal/create (1x)"), "{message}");
        assert!(!message.contains("fs/read_text_file"), "{message}");
    }

    #[test]
    fn counts_blocked_methods_from_log_lines() {
        let log = "2026-10-21T08:00:00Z  INFO launch agent=claude\n\
                   2026-10-21T08:00:01Z  INFO blocked method=fs/read_text_file dir=agent_to_client\n\
                   2026-10-21T08:00:02Z  INFO blocked method=fs/read_text_file dir=agent_to_client\n\
                   2026-10-21T08:00:03Z  INFO blocked method=terminal/create dir=agent_to_client (ignored: x)\n";
        let counts = count_blocked_methods(&[log.to_string()]);
        assert_eq!(counts.get("fs/read_text_file"), Some(&2));
        assert_eq!(counts.get("terminal/create"), Some(&1));
        assert_eq!(counts.len(), 2);
        assert!(count_blocked_methods(&["nothing here".to_string()]).is_empty());
    }

    #[test]
    fn agent_line_priorities() {
        // Missing image: launch would fail → FAIL, even if signed in.
        assert_eq!(
            agent_check("claude", "r1", Some(false), true, false).0,
            "FAIL"
        );
        // Not signed in → WARN with the fix, and the release is still shown.
        let (s, m) = agent_check("antigravity", "r2", Some(true), false, false);
        assert_eq!(s, "WARN");
        assert!(
            m.contains("release r2 (not authenticated)") && m.contains("zevbox auth antigravity"),
            "{m}"
        );
        // Missing model keeps release and auth state in the message.
        let (s, m) = agent_check("claude", "r3", Some(true), true, true);
        assert_eq!(s, "WARN");
        assert!(
            m.contains("release r3 (authenticated)") && m.contains("no default model"),
            "{m}"
        );
        // Runtime down (image unknown) does not fail the agent.
        assert_eq!(agent_check("claude", "r4", None, true, false).0, "OK");
        assert_eq!(agent_check("claude", "r5", Some(true), true, false).0, "OK");
    }

    #[test]
    fn disk_message_warns_only_with_old_images() {
        let (status, msg) = disk_message(2, 0, Some(61_615_472_640));
        assert_eq!(status, "OK");
        assert!(
            msg.contains("2 zevbox image(s)") && msg.contains("61.6 GB"),
            "{msg}"
        );
        let (status, msg) = disk_message(5, 3, None);
        assert_eq!(status, "WARN");
        assert!(
            msg.contains("3 from old releases") && msg.contains("zevbox clean --images"),
            "{msg}"
        );
    }
}

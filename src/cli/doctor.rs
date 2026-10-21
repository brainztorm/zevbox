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
    }

    // 6. Disk used by zevbox images (DECISIONS G1)
    if let Ok(images) = cli.image_list().await {
        let zevbox = images
            .iter()
            .filter(|i| crate::container::is_zevbox_image(&i.reference))
            .count();
        let keep = crate::release::images_to_keep(&paths);
        let stale = crate::container::stale_zevbox_images(&images, &keep).len();
        let (status, message) = disk_message(zevbox, stale, cli.images_bytes().await);
        checks.push(("disk".to_string(), status.to_string(), message));
    }

    // 7. Permissions 0700 on stateRoot and state/<agent> (plan §8.6 point 6)
    checks.push(permissions_check(&paths));

    // 8. Installed binary and Zed entries (plan §8.6 points 7 and 8)
    checks.extend(zed_checks(&paths));

    // 9. Orphan or unregistered instances (plan §8.6 point 9)
    if let Ok(containers) = cli.list_all().await {
        checks.push(instances_check(&paths, &containers));
    }

    finish(json_output, &checks)
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
    dirs.extend(crate::agents::AGENTS.iter().map(|a| paths.state_dir(a.id)));
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
        .filter(|c| {
            crate::container::may_target(c) && !records.iter().any(|r| r.container_name == c.name)
        })
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
    use super::{agent_check, disk_message};

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

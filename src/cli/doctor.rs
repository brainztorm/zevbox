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
            checks.push((
                "container_binary".to_string(),
                "OK".to_string(),
                format!("Apple Container {} ({})", sys.version, sys.architecture),
            ));

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

    // 5. Active agents status
    let active = crate::release::load_active(&paths).unwrap_or_default();
    for spec in crate::agents::AGENTS {
        if let Some(entry) = active.get(spec.id) {
            match crate::release::load_lock(&paths, &entry.current) {
                Ok(lock) => {
                    let marker_path = paths.state_dir(spec.id).join(&lock.auth_marker);
                    let auth_status = if marker_path.exists() {
                        "authenticated"
                    } else {
                        "not authenticated"
                    };
                    let missing_model = spec.id == "claude" && !has_default_model(&paths);
                    if missing_model {
                        checks.push((
                            format!("agent:{}", spec.id),
                            "WARN".to_string(),
                            "no default model in state/claude/.claude/settings.json (Auto mode falls back to Accept edits in Zed). Fix: run 'zevbox auth claude' or write {\"model\": \"fable\"} in that file".to_string(),
                        ));
                    } else {
                        checks.push((
                            format!("agent:{}", spec.id),
                            "OK".to_string(),
                            format!("release {} ({auth_status})", entry.current),
                        ));
                    }
                }
                Err(e) => {
                    checks.push((
                        format!("agent:{}", spec.id),
                        "FAIL".to_string(),
                        format!("corrupt lock for {}: {e}", entry.current),
                    ));
                }
            }
        } else {
            checks.push((
                format!("agent:{}", spec.id),
                "WARN".to_string(),
                "no active release installed".to_string(),
            ));
        }
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

    // 7. Permissions 0700 on stateRoot
    match std::fs::metadata(&paths.state_root) {
        Ok(m) => {
            use std::os::unix::fs::PermissionsExt;
            let perm = m.permissions().mode() & 0o777;
            if perm == 0o700 {
                checks.push((
                    "permissions".to_string(),
                    "OK".to_string(),
                    "stateRoot is 0700".to_string(),
                ));
            } else {
                checks.push((
                    "permissions".to_string(),
                    "WARN".to_string(),
                    format!("stateRoot mode is 0o{perm:o}, expected 0700"),
                ));
            }
        }
        Err(e) => {
            checks.push((
                "permissions".to_string(),
                "FAIL".to_string(),
                format!("cannot check stateRoot: {e}"),
            ));
        }
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
    use super::disk_message;

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

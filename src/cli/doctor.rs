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

    // 4. Projects exist and canonical
    for p in &config.projects {
        if p.exists() {
            checks.push((
                format!("project:{}", p.display()),
                "OK".to_string(),
                "project directory exists".to_string(),
            ));
        } else {
            checks.push((
                format!("project:{}", p.display()),
                "FAIL".to_string(),
                "project directory does not exist".to_string(),
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
                    checks.push((
                        format!("agent:{}", spec.id),
                        "OK".to_string(),
                        format!("release {} ({auth_status})", entry.current),
                    ));
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

    // 6. Permissions 0700 on stateRoot
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

//! `zevbox update` and `zevbox rollback` (§8.1, §8.7).

use super::AgentArg;
use crate::fsutil::sha256_hex;
use crate::paths::Paths;
use crate::release::{
    build_image, fetch_manifest, load_active, new_release_id, parse_antigravity_manifest,
    parse_claude_manifest, prepare_context, promote, rollback, run_smoke_test, save_active,
    save_lock, ReleaseLock, BASE_IMAGE,
};
use crate::supervisor::{EXIT_ERROR, EXIT_OK, EXIT_PRECONDITION, EXIT_USAGE};

pub async fn run_update(check: bool, agent_arg: Option<AgentArg>) -> anyhow::Result<u8> {
    let paths = Paths::from_env()?;
    if !paths.config_file().exists() {
        crate::ui::err(
            "zevbox is not set up. Fix: run 'zevbox setup' or 'zevbox init' in a macOS terminal.",
        );
        return Ok(EXIT_PRECONDITION);
    }
    let config = crate::config::load(&paths.config_file())?;
    let tools = crate::tools::load(&paths.tools_file()).unwrap_or_default();

    let target_agents: Vec<&'static crate::agents::AgentSpec> = match agent_arg {
        Some(a) => match crate::agents::find(a.id()) {
            Some(spec) => vec![spec],
            None => vec![],
        },
        None => crate::agents::AGENTS.iter().collect(),
    };

    let mut active = load_active(&paths)?;
    // A build happened: old images and the builder VM can be reclaimed (G1).
    let mut built = false;

    for agent_spec in target_agents {
        let (url, sha, text) = match fetch_manifest(agent_spec.registry_id) {
            Ok(res) => res,
            Err(e) => {
                crate::ui::err(&format!(
                    "{}: cannot fetch manifest: {e}",
                    agent_spec.display
                ));
                continue;
            }
        };

        let (version, npm_pkg, archive_url, agent_cmd, auth_cmd, auth_marker) =
            if agent_spec.id == "claude" {
                let c = match parse_claude_manifest(&text) {
                    Ok(c) => c,
                    Err(e) => {
                        crate::ui::err(&format!("{}: invalid manifest: {e}", agent_spec.display));
                        continue;
                    }
                };
                (
                    c.version,
                    Some(c.npm_package),
                    None,
                    vec!["/opt/zevbox/claude/node_modules/.bin/claude-agent-acp".to_string()],
                    vec![
                        "/opt/zevbox/claude/node_modules/.bin/claude-agent-acp".to_string(),
                        "--cli".to_string(),
                        "auth".to_string(),
                        "login".to_string(),
                        "--claudeai".to_string(),
                    ],
                    ".claude/.credentials.json".to_string(),
                )
            } else {
                let a = match parse_antigravity_manifest(&text) {
                    Ok(a) => a,
                    Err(e) => {
                        crate::ui::err(&format!("{}: invalid manifest: {e}", agent_spec.display));
                        continue;
                    }
                };
                (
                    a.version,
                    None,
                    Some(a.archive_url),
                    vec!["/opt/zevbox/antigravity/agy_acp_server.par".to_string()],
                    vec!["/opt/zevbox/antigravity/agy_acp_server.par".to_string()],
                    ".gemini/antigravity-acp/acp_token.json".to_string(),
                )
            };

        if check {
            let current_version = active.get(agent_spec.id).and_then(|e| {
                crate::release::load_lock(&paths, &e.current)
                    .ok()
                    .map(|l| l.agent_version)
            });
            match current_version {
                Some(v) if v == version => {
                    crate::ui::out(&format!("{}: up to date ({version})", agent_spec.display));
                }
                Some(v) => {
                    crate::ui::out(&format!(
                        "{}: update available ({} -> {})",
                        agent_spec.display, v, version
                    ));
                }
                None => {
                    crate::ui::out(&format!(
                        "{}: not installed (available: {})",
                        agent_spec.display, version
                    ));
                }
            }
            continue;
        }

        crate::ui::out(&format!(
            "{}: preparing release for version {}...",
            agent_spec.display, version
        ));
        built = true;
        let now = chrono::Utc::now();
        let release_id = new_release_id(agent_spec.id, now);
        let image = format!("zevbox/{}:{release_id}", agent_spec.id);
        let context_dir = paths.release_dir(&release_id).join("context");
        let build_log = paths.release_dir(&release_id).join("build.log");

        let mut lock = ReleaseLock {
            schema_version: 1,
            release_id: release_id.clone(),
            agent: agent_spec.id.to_string(),
            created_at: now.to_rfc3339(),
            registry_manifest_url: url,
            registry_manifest_sha256: sha,
            agent_version: version.clone(),
            base_image: BASE_IMAGE.to_string(),
            apt_packages: tools.apt.clone(),
            npm_package: npm_pkg,
            archive_url: archive_url.clone(),
            archive_sha256: None,
            agent_command: agent_cmd,
            auth_command: auth_cmd,
            auth_marker,
            image: image.clone(),
            smoke_test: None,
        };

        if let Err(e) = prepare_context(&context_dir, &lock, &config.container_binary) {
            crate::ui::err(&format!(
                "{}: prepare context failed: {e}",
                agent_spec.display
            ));
            continue;
        }

        if agent_spec.id == "antigravity" {
            let agy_zip = context_dir.join("agy.zip");
            if agy_zip.exists() {
                if let Ok(bytes) = std::fs::read(&agy_zip) {
                    lock.archive_sha256 = Some(sha256_hex(&bytes));
                }
            }
        }

        crate::ui::out(&format!(
            "{}: building image {}...",
            agent_spec.display, image
        ));
        if let Err(e) = build_image(
            &context_dir,
            &lock,
            &config.container_binary,
            &build_log,
            config.host_uid,
            config.host_gid,
        ) {
            crate::ui::err(&format!("{}: build failed: {e}", agent_spec.display));
            continue;
        }

        crate::ui::out(&format!("{}: running smoke test...", agent_spec.display));
        match run_smoke_test(&lock, &config.container_binary) {
            Ok(smoke) => {
                lock.smoke_test = Some(smoke);
                if let Err(e) = save_lock(&paths, &release_id, &lock) {
                    crate::ui::err(&format!("{}: cannot save lock: {e}", agent_spec.display));
                    continue;
                }
                promote(&mut active, agent_spec.id, &release_id);
                if let Err(e) = save_active(&paths, &active) {
                    crate::ui::err(&format!(
                        "{}: cannot save active release: {e}",
                        agent_spec.display
                    ));
                    continue;
                }
                crate::ui::out(&format!(
                    "{}: promoted to {}",
                    agent_spec.display, release_id
                ));
                let _ = crate::cli::zed::update_zed(&paths, &config);
            }
            Err(e) => {
                crate::ui::err(&format!("{}: smoke test failed: {e}", agent_spec.display));
                let cli = crate::container::ContainerCli::new(config.container_binary.clone());
                if let Err(e) = cli.delete_zevbox_image(&image).await {
                    tracing::warn!("cannot delete failed image {image}: {e}");
                }
            }
        }
    }

    if built {
        let cli = crate::container::ContainerCli::new(config.container_binary.clone());
        for reference in crate::release::prune_images(&paths, &cli).await {
            crate::ui::out(&format!("removed old image {reference}"));
        }
    }

    Ok(EXIT_OK)
}

pub async fn run_rollback(agent_arg: Option<AgentArg>) -> anyhow::Result<u8> {
    let paths = Paths::from_env()?;
    if !paths.config_file().exists() {
        crate::ui::err(
            "zevbox is not set up. Fix: run 'zevbox setup' or 'zevbox init' in a macOS terminal.",
        );
        return Ok(EXIT_PRECONDITION);
    }
    let Some(agent_arg) = agent_arg else {
        crate::ui::err("zevbox: --agent is required for rollback");
        return Ok(EXIT_USAGE);
    };

    let mut active = load_active(&paths)?;
    if let Err(e) = rollback(&mut active, agent_arg.id()) {
        crate::ui::err(&format!("zevbox: {e}"));
        return Ok(EXIT_ERROR);
    }
    save_active(&paths, &active)?;
    let now_current = active
        .get(agent_arg.id())
        .map(|e| e.current.clone())
        .unwrap_or_default();
    crate::ui::out(&format!("{}: rolled back to {now_current}", agent_arg.id()));

    let config = crate::config::load(&paths.config_file())?;
    let _ = crate::cli::zed::update_zed(&paths, &config);

    Ok(EXIT_OK)
}

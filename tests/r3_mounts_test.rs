//! DECISIONS R3 — auth/shell never mount an unauthorized directory; `setup`
//! is non-interactive without a terminal. Not in the protected manifest.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::TestEnv;
use zevbox::cli::auth::auth_run_spec;
use zevbox::config::load;
use zevbox::container::run_args;
use zevbox::paths::Paths;
use zevbox::release::active_lock;

async fn run_in(env: &TestEnv, cwd: &std::path::Path, args: &[&str]) -> std::process::Output {
    let mut cmd = tokio::process::Command::new(common::bin());
    cmd.env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &env.home)
        .env("ZEVBOX_STATE_ROOT", &env.state_root)
        .env("ZEVBOX_ZED_SETTINGS", &env.zed_settings)
        .env("ZEVBOX_INSTALLED_BINARY", &env.installed_binary)
        .env("FAKE_CONTAINER_DIR", &env.fake_dir)
        .env("FAKE_CONTAINER_LOG", &env.container_log)
        .current_dir(cwd)
        .args(args)
        .stdin(std::process::Stdio::null());
    cmd.output().await.unwrap()
}

fn add_antigravity_release(env: &TestEnv) {
    let releases = env.state_root.join("releases");
    let mut lock: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(releases.join("claude-test/lock.json")).unwrap(),
    )
    .unwrap();
    lock["releaseId"] = "antigravity-test".into();
    lock["agent"] = "antigravity".into();
    lock["authMarker"] = ".gemini/antigravity-acp/acp_token.json".into();
    lock["image"] = "zevbox/antigravity:antigravity-test".into();
    std::fs::create_dir_all(releases.join("antigravity-test")).unwrap();
    common::write_json(&releases.join("antigravity-test/lock.json"), &lock);
    common::write_json(
        &releases.join("active.json"),
        &serde_json::json!({
            "claude": {"current": common::RELEASE, "previous": null},
            "antigravity": {"current": "antigravity-test", "previous": null}
        }),
    );
}

#[tokio::test]
async fn shell_outside_a_project_is_refused() {
    let env = TestEnv::new();
    let out = run_in(&env, &env.home, &["shell", "claude"]).await;
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("is not an allowed project"));
    assert!(!env.container_calls().iter().any(|c| c.starts_with("run ")));
}

#[test]
fn auth_mounts_only_an_empty_dir_and_the_state_dir() {
    let env = TestEnv::new();
    let paths = Paths::new(&env.state_root);
    let config = load(&paths.config_file()).unwrap();
    let lock = active_lock(&paths, "claude").unwrap();
    let tmp = env.root.join("empty");
    std::fs::create_dir_all(&tmp).unwrap();
    let args = run_args(&auth_run_spec(&config, &paths, &lock, &tmp));
    let sources: Vec<String> = args
        .iter()
        .flat_map(|a| a.split(',').map(str::to_string).collect::<Vec<_>>())
        .filter_map(|p| p.strip_prefix("source=").map(str::to_string))
        .collect();
    assert_eq!(sources.len(), 2, "{args:?}");
    assert!(sources.contains(&tmp.display().to_string()));
    assert!(sources.contains(&paths.state_dir("claude").display().to_string()));
}

#[tokio::test]
async fn setup_without_terminal_skips_authentication() {
    let env = TestEnv::new();
    std::fs::remove_file(
        env.state_root
            .join("state/claude/.claude/.credentials.json"),
    )
    .unwrap();
    add_antigravity_release(&env); // keeps `setup` from trying to build (network)
    let out = env.run(&["setup"]).await;
    let stdout = out.stdout;
    assert!(stdout.contains("skipping authentication"), "{stdout}");
    assert!(!env.container_calls().iter().any(|c| c.contains(" -it ")));
}

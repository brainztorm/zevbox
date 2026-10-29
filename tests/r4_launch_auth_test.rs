//! DECISIONS backlog 4 — `launch` tests authentication like `auth --status` and
//! `doctor`: for Antigravity, `acp_token.json` must hold a non-empty refresh token,
//! not merely exist. Not in the protected manifest.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{bin, manifest_dir, write_json, Launch, TestEnv, IMAGE, RELEASE};
use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};

const MARKER: &str = ".gemini/antigravity-acp/acp_token.json";

/// Adds an active `antigravity` release (same fake image as Claude).
fn add_antigravity_release(env: &TestEnv) {
    let releases = env.state_root.join("releases");
    let mut lock: Value = serde_json::from_str(
        &std::fs::read_to_string(releases.join("claude-test/lock.json")).unwrap(),
    )
    .unwrap();
    lock["releaseId"] = "antigravity-test".into();
    lock["agent"] = "antigravity".into();
    lock["authMarker"] = MARKER.into();
    lock["image"] = IMAGE.into();
    std::fs::create_dir_all(releases.join("antigravity-test")).unwrap();
    write_json(&releases.join("antigravity-test/lock.json"), &lock);
    write_json(
        &releases.join("active.json"),
        &json!({
            "claude": {"current": RELEASE, "previous": null},
            "antigravity": {"current": "antigravity-test", "previous": null}
        }),
    );
}

fn write_token(env: &TestEnv, token: &Value) {
    let path = env.state_root.join("state/antigravity").join(MARKER);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_json(&path, token);
}

fn launch_antigravity(env: &TestEnv) -> Launch {
    let mut cmd = tokio::process::Command::new(bin());
    cmd.env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &env.home)
        .env("ZEVBOX_STATE_ROOT", &env.state_root)
        .env("ZEVBOX_ZED_SETTINGS", &env.zed_settings)
        .env("ZEVBOX_INSTALLED_BINARY", &env.installed_binary)
        .env("FAKE_CONTAINER_DIR", &env.fake_dir)
        .env("FAKE_CONTAINER_LOG", &env.container_log)
        .env(
            "FAKE_AGENT",
            manifest_dir().join("tests/fixtures/fake-agent.py"),
        )
        .env("FAKE_AGENT_LOG", &env.agent_log)
        .env("FAKE_AGENT_MODE", "normal")
        .args(["launch", "antigravity", "--project"])
        .arg(&env.project)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    let mut child = cmd.spawn().unwrap();
    let stdin = child.stdin.take();
    let stdout = BufReader::new(child.stdout.take().unwrap()).lines();
    Launch {
        child,
        stdin,
        stdout,
    }
}

#[tokio::test]
async fn token_file_without_refresh_token_is_not_authenticated() {
    let env = TestEnv::new();
    add_antigravity_release(&env);
    write_token(&env, &json!({"access_token": "a", "refresh_token": ""}));
    let mut l = launch_antigravity(&env);
    let r = l.initialize().await;
    let msg = r["error"]["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("Antigravity is not authenticated. Fix: run 'zevbox auth antigravity'"),
        "{r}"
    );
    let (code, _) = l.wait(Duration::from_secs(10)).await;
    assert_eq!(code, Some(3));
    assert!(!env.container_calls().iter().any(|c| c.starts_with("run ")));
}

#[tokio::test]
async fn token_file_with_refresh_token_starts_the_agent() {
    let env = TestEnv::new();
    add_antigravity_release(&env);
    write_token(&env, &json!({"refresh_token": "r-1"}));
    let mut l = launch_antigravity(&env);
    let r = l.initialize().await;
    assert!(r.get("result").is_some(), "{r}");
    assert!(env.container_calls().iter().any(|c| c.starts_with("run ")));
    l.close_stdin();
    l.wait(Duration::from_secs(20)).await;
}

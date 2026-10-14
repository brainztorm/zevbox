//! DECISIONS R1 (2026-10-14) — `zevbox launch <agent>` without `--project`
//! takes the project from the working directory, as Zed starts the agent in
//! the project's first worktree. Not in the protected manifest.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{bin, manifest_dir, Launch, TestEnv};
use serde_json::json;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;

/// Like `TestEnv::launch_with`, but no `--project` and a chosen working directory.
fn launch_in(env: &TestEnv, cwd: &std::path::Path) -> Launch {
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
        .current_dir(cwd)
        .args(["launch", "claude"])
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
async fn launch_without_project_uses_allowed_cwd() {
    let env = TestEnv::new();
    let mut l = launch_in(&env, &env.project);
    let r = l.initialize().await;
    assert_eq!(r["id"], 0);
    assert!(r.get("result").is_some(), "initialize must succeed: {r}");
    l.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "session/new",
        "params": {"cwd": env.project, "mcpServers": []}
    }))
    .await;
    let s = l.recv().await;
    assert_eq!(s["id"], 1);
    assert!(
        s.get("result").is_some(),
        "session/new in the cwd project: {s}"
    );
    assert!(
        env.container_calls().iter().any(|c| c.starts_with("run ")),
        "a container must start"
    );
    l.close_stdin();
    let (code, _) = l.wait(Duration::from_secs(15)).await;
    assert_eq!(code, Some(0));
}

#[tokio::test]
async fn launch_without_project_refuses_unknown_cwd() {
    let env = TestEnv::new();
    let other = env.home.join("code/other");
    std::fs::create_dir_all(&other).unwrap();
    let mut l = launch_in(&env, &other);
    let r = l.initialize().await;
    assert_eq!(r["error"]["code"], -32000, "{r}");
    let msg = r["error"]["message"].as_str().unwrap();
    assert!(msg.contains("is not an allowed project"), "{msg}");
    assert!(
        msg.contains(&format!("zevbox project add {}", other.display())),
        "{msg}"
    );
    let (code, _) = l.wait(Duration::from_secs(5)).await;
    assert_eq!(code, Some(3));
    assert!(
        !env.container_calls().iter().any(|c| c.starts_with("run ")),
        "no container may start"
    );
}

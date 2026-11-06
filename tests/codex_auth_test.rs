//! `zevbox auth codex`, its status and the login VM. Not in the protected manifest.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{bin, manifest_dir, write_json, Launch, TestEnv, IMAGE, RELEASE};
use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};

/// Adds an active Codex release (same fake image as Claude).
fn add_codex_release(env: &TestEnv) {
    let releases = env.state_root.join("releases");
    let mut lock: Value = serde_json::from_str(
        &std::fs::read_to_string(releases.join("claude-test/lock.json")).unwrap(),
    )
    .unwrap();
    lock["releaseId"] = "codex-test".into();
    lock["agent"] = "codex".into();
    lock["authCommand"] = json!([
        "env",
        "CODEX_HOME=/home/agent/.codex-login",
        "/x",
        "cli",
        "login"
    ]);
    lock["authMarker"] = ".codex/auth.json".into();
    lock["image"] = IMAGE.into();
    std::fs::create_dir_all(releases.join("codex-test")).unwrap();
    write_json(&releases.join("codex-test/lock.json"), &lock);
    write_json(
        &releases.join("active.json"),
        &json!({
            "claude": {"current": RELEASE, "previous": null},
            "codex": {"current": "codex-test", "previous": null}
        }),
    );
}

fn write_cache(env: &TestEnv, text: &str) {
    let dir = env.state_root.join("state/codex/.codex");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("auth.json"), text).unwrap();
}

async fn status(env: &TestEnv) -> String {
    let out = env.run(&["auth", "codex", "--status"]).await;
    out.stdout.trim().to_string()
}

const CHATGPT: &str = r#"{"auth_mode":"chatgpt","OPENAI_API_KEY":null,"tokens":{"id_token":"a.b.c","access_token":"t","refresh_token":"r"},"last_refresh":"2026-11-06T10:00:00Z"}"#;
const API: &str = r#"{"auth_mode":"apikey","OPENAI_API_KEY":"sk-synthetic"}"#;

#[tokio::test]
async fn native_caches_are_recognised() {
    let env = TestEnv::new();
    add_codex_release(&env);
    assert_eq!(status(&env).await, "not authenticated");
    write_cache(&env, CHATGPT);
    assert_eq!(status(&env).await, "authenticated");
    write_cache(&env, API);
    assert_eq!(status(&env).await, "authenticated");
}

#[tokio::test]
async fn broken_or_unknown_caches_are_refused() {
    let env = TestEnv::new();
    add_codex_release(&env);
    for bad in [
        "",
        "{",
        "{}",
        r#"{"auth_mode":"apikey","OPENAI_API_KEY":""}"#,
        r#"{"auth_mode":"apikey","OPENAI_API_KEY":null}"#,
        r#"{"auth_mode":"chatgpt","tokens":{"id_token":"a","access_token":"","refresh_token":"r"}}"#,
        r#"{"auth_mode":"chatgpt"}"#,
        r#"{"auth_mode":"headers"}"#,
    ] {
        write_cache(&env, bad);
        assert_eq!(status(&env).await, "not authenticated", "{bad}");
    }
}

#[tokio::test]
async fn status_without_config_is_unknown_and_creates_nothing() {
    let env = TestEnv::bare();
    let out = env.run(&["auth", "codex", "--status"]).await;
    assert_eq!(out.stdout.trim(), "unknown");
    assert!(!env.state_root.join("config.json").exists());
}

#[tokio::test]
async fn failed_login_keeps_the_current_account_and_touches_no_other_agent() {
    let env = TestEnv::new();
    add_codex_release(&env);
    write_cache(&env, CHATGPT);
    let claude_before = std::fs::read_dir(env.state_root.join("state/claude/.claude"))
        .unwrap()
        .count();
    // The fake VM writes no new login, so the sign-in does not finish.
    let out = env.run(&["auth", "codex"]).await;
    assert_eq!(out.code, Some(1), "{}", out.stderr);
    assert!(out.stderr.contains("did not finish"), "{}", out.stderr);
    let cache = env.state_root.join("state/codex/.codex/auth.json");
    assert_eq!(std::fs::read_to_string(cache).unwrap(), CHATGPT);
    assert!(!env.state_root.join("state/codex/.codex-login").exists());
    assert_eq!(
        std::fs::read_dir(env.state_root.join("state/claude/.claude"))
            .unwrap()
            .count(),
        claude_before
    );
    let calls = env.container_calls();
    let run = calls.iter().find(|c| c.starts_with("run ")).unwrap();
    assert!(run.contains("--name zevbox-codex-sh-"), "{run}");
    assert!(run.contains("zevbox.agent=codex"), "{run}");
    assert!(run.contains("/state/codex,target=/home/agent"), "{run}");
    assert!(run.contains("CODEX_HOME=/home/agent/.codex-login"), "{run}");
    assert!(!run.contains("/state/claude"), "{run}");
}

/// `zevbox launch codex --project <project>`, like Zed.
fn launch_codex(env: &TestEnv) -> Launch {
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
        .args(["launch", "codex", "--project"])
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
async fn launch_without_login_tells_zed_what_to_run() {
    let env = TestEnv::new();
    add_codex_release(&env);
    let mut l = launch_codex(&env);
    let r = l.initialize().await;
    let msg = r["error"]["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("Codex is not authenticated. Fix: run 'zevbox auth codex'"),
        "{r}"
    );
    let (code, _) = l.wait(Duration::from_secs(10)).await;
    assert_eq!(code, Some(3));
    assert!(!env.container_calls().iter().any(|c| c.starts_with("run ")));
}

#[tokio::test]
async fn launch_mounts_its_own_state() {
    let env = TestEnv::new();
    add_codex_release(&env);
    write_cache(&env, CHATGPT);
    let mut l = launch_codex(&env);
    let r = l.initialize().await;
    assert!(r["result"].is_object(), "{r}");
    l.close_stdin();
    let _ = l.wait(Duration::from_secs(10)).await;
    let calls = env.container_calls();
    let run = calls.iter().find(|c| c.starts_with("run ")).unwrap();
    assert!(run.contains("--name zevbox-codex-"), "{run}");
    assert!(run.contains("/state/codex,target=/home/agent"), "{run}");
    assert!(!run.contains("/state/claude"), "{run}");
}

#[tokio::test]
async fn zed_entry_appears_once_installed() {
    let env = TestEnv::new();
    let read_servers = |env: &TestEnv| {
        let text = std::fs::read_to_string(&env.zed_settings).unwrap_or_default();
        let v: Value = jsonc_parser::parse_to_serde_value(&text, &Default::default()).unwrap();
        v["agent_servers"].clone()
    };
    assert_eq!(env.run(&["zed", "install"]).await.code, Some(0));
    assert!(read_servers(&env).get("zevbox · Codex").is_none());
    add_codex_release(&env);
    assert_eq!(env.run(&["zed", "install"]).await.code, Some(0));
    assert_eq!(
        read_servers(&env)["zevbox · Codex"]["args"],
        json!(["launch", "codex"])
    );
}

#[tokio::test]
async fn doctor_treats_codex_as_a_standard_agent() {
    let env = TestEnv::new();
    let out = env.run(&["doctor", "--json"]).await;
    let checks: Value = serde_json::from_str(&out.stdout).unwrap();
    let codex = checks
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["check"] == "agent:codex")
        .unwrap_or_else(|| panic!("no agent:codex check: {checks}"));
    assert_eq!(codex["status"], "WARN", "{codex}");
    assert!(codex["message"]
        .as_str()
        .unwrap()
        .contains("no active release"));

    add_codex_release(&env);
    let out = env.run(&["doctor"]).await;
    assert!(
        out.stdout.contains("Fix: run 'zevbox auth codex'"),
        "{}",
        out.stdout
    );
    assert!(!out.stdout.contains("no default model in state/codex"));
}

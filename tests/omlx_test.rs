//! DECISIONS 2026-10-29 — oMLX agent: the Claude adapter in its own VM and state
//! folder, pointed at the user's inference server by `zevbox auth omlx`.
//! Not in the protected manifest.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{bin, manifest_dir, write_json, Launch, TestEnv, IMAGE, RELEASE};
use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};

const KEY: &str = "omlx-secret-test-key-4242";

/// Adds an active `omlx` release (same fake image as Claude).
fn add_omlx_release(env: &TestEnv) {
    let releases = env.state_root.join("releases");
    let mut lock: Value = serde_json::from_str(
        &std::fs::read_to_string(releases.join("claude-test/lock.json")).unwrap(),
    )
    .unwrap();
    lock["releaseId"] = "omlx-test".into();
    lock["agent"] = "omlx".into();
    lock["authCommand"] = json!([]);
    lock["authMarker"] = ".claude/settings.json".into();
    lock["image"] = IMAGE.into();
    std::fs::create_dir_all(releases.join("omlx-test")).unwrap();
    write_json(&releases.join("omlx-test/lock.json"), &lock);
    write_json(
        &releases.join("active.json"),
        &json!({
            "claude": {"current": RELEASE, "previous": null},
            "omlx": {"current": "omlx-test", "previous": null}
        }),
    );
}

fn write_settings(env: &TestEnv, settings: &Value) {
    let dir = env.state_root.join("state/omlx/.claude");
    std::fs::create_dir_all(&dir).unwrap();
    write_json(&dir.join("settings.json"), settings);
}

fn server_settings() -> Value {
    json!({
        "model": "Small",
        "env": {"ANTHROPIC_BASE_URL": "http://192.168.1.8:8000", "ANTHROPIC_AUTH_TOKEN": KEY}
    })
}

/// `zevbox launch omlx --project <project>`, like Zed.
fn launch_omlx(env: &TestEnv) -> Launch {
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
        .args(["launch", "omlx", "--project"])
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

async fn status(env: &TestEnv) -> String {
    let out = env.run(&["auth", "omlx", "--status"]).await;
    out.stdout.trim().to_string()
}

#[tokio::test]
async fn auth_status_needs_a_server_and_a_key() {
    let env = TestEnv::new();
    add_omlx_release(&env);
    assert_eq!(status(&env).await, "not authenticated");
    write_settings(&env, &json!({"model": "Small"}));
    assert_eq!(status(&env).await, "not authenticated");
    write_settings(&env, &server_settings());
    assert_eq!(status(&env).await, "authenticated");
}

#[tokio::test]
async fn server_options_are_refused_for_other_agents() {
    let env = TestEnv::new();
    let out = env
        .run(&["auth", "claude", "--url", "http://192.168.1.8:8000"])
        .await;
    assert_eq!(out.code, Some(2), "{}", out.stderr);
    assert!(out.stderr.contains("only apply to 'zevbox auth omlx'"));
}

#[tokio::test]
async fn launch_without_server_settings_tells_zed_what_to_run() {
    let env = TestEnv::new();
    add_omlx_release(&env);
    write_settings(&env, &json!({"model": "Small"}));
    let mut l = launch_omlx(&env);
    let r = l.initialize().await;
    let msg = r["error"]["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("oMLX is not authenticated. Fix: run 'zevbox auth omlx'"),
        "{r}"
    );
    let (code, _) = l.wait(Duration::from_secs(10)).await;
    assert_eq!(code, Some(3));
    assert!(!env.container_calls().iter().any(|c| c.starts_with("run ")));
}

#[tokio::test]
async fn launch_mounts_its_own_state_and_never_passes_the_key() {
    let env = TestEnv::new();
    add_omlx_release(&env);
    write_settings(&env, &server_settings());
    let mut l = launch_omlx(&env);
    let r = l.initialize().await;
    assert!(r.get("result").is_some(), "{r}");
    l.close_stdin();
    let (code, _) = l.wait(Duration::from_secs(15)).await;
    assert_eq!(code, Some(0));

    let run = env
        .container_calls()
        .into_iter()
        .find(|c| c.starts_with("run "))
        .expect("a container must start");
    let state = env.state_root.join("state/omlx");
    assert!(
        run.contains(&format!(
            "type=bind,source={},target=/home/agent",
            state.display()
        )),
        "{run}"
    );
    assert!(run.contains("--name zevbox-omlx-"), "{run}");
    assert!(!run.contains("state/claude"), "{run}");
    // I9: the key lives in the agent's own settings file only.
    assert!(!run.contains(KEY), "{run}");
    for entry in std::fs::read_dir(env.state_root.join("logs")).unwrap() {
        let text = std::fs::read_to_string(entry.unwrap().path()).unwrap_or_default();
        assert!(!text.contains(KEY), "key leaked into a log");
    }
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
    assert!(read_servers(&env).get("zevbox · oMLX").is_none());
    add_omlx_release(&env);
    assert_eq!(env.run(&["zed", "install"]).await.code, Some(0));
    assert_eq!(
        read_servers(&env)["zevbox · oMLX"]["args"],
        json!(["launch", "omlx"])
    );
}

#[tokio::test]
async fn doctor_calls_an_uninstalled_omlx_optional() {
    let env = TestEnv::new();
    let out = env.run(&["doctor", "--json"]).await;
    let checks: Value = serde_json::from_str(&out.stdout).unwrap();
    let omlx = checks
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["check"] == "agent:omlx")
        .unwrap_or_else(|| panic!("no agent:omlx check: {checks}"));
    assert_eq!(omlx["status"], "OK", "{omlx}");
    assert!(omlx["message"]
        .as_str()
        .unwrap()
        .contains("optional, not installed"));
}

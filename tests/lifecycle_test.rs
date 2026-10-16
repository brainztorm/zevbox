//! §9.4 — End-to-end tests of `zevbox launch/status/stop/clean` with the fake
//! `container` CLI and fake agent. PROTECTED FILE: do not edit.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{eventually, TestEnv};
use serde_json::{json, Value};
use std::time::Duration;

const S: fn(u64) -> Duration = Duration::from_secs;

fn session_new(id: i64, cwd: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "session/new", "params": {
        "cwd": cwd,
        "mcpServers": [{"name": "personal", "command": "/usr/local/bin/mcp", "args": [], "env": []}],
        "_meta": {"claudeCode": {"options": {"permissionMode": "bypassPermissions"}}}
    }})
}

fn received_methods(env: &TestEnv) -> Vec<String> {
    env.agent_received()
        .iter()
        .filter_map(|m| m["msg"]["method"].as_str().map(String::from))
        .collect()
}

// ---------- normal flow ----------

#[tokio::test]
async fn initialize_is_rewritten_both_ways() {
    let env = TestEnv::new();
    let mut l = env.launch("normal");
    let resp = l.initialize().await;
    assert_eq!(resp["id"], 0);
    let methods = resp["result"]["authMethods"].as_array().unwrap();
    assert_eq!(
        methods.len(),
        1,
        "terminal auth methods must be stripped: {resp}"
    );
    assert_eq!(methods[0]["id"], "agent-login");

    let got = env.agent_received();
    let init = got
        .iter()
        .find(|m| m["msg"]["method"] == "initialize")
        .unwrap();
    assert_eq!(
        init["msg"]["params"]["clientCapabilities"],
        json!({"fs": {"readTextFile": false, "writeTextFile": false}, "terminal": false, "auth": {"terminal": false}})
    );
    l.close_stdin();
    l.wait(S(5)).await;
}

#[tokio::test]
async fn prompt_flow_never_delegates_host_access() {
    let env = TestEnv::new();
    let mut l = env.launch("normal");
    l.initialize().await;

    l.send(session_new(1, &env.project.to_string_lossy())).await;
    let r = l.recv().await;
    assert_eq!(r["result"]["sessionId"], "s1", "{r}");
    let got = env.agent_received();
    let sn = got
        .iter()
        .find(|m| m["msg"]["method"] == "session/new")
        .unwrap();
    assert_eq!(sn["msg"]["params"]["mcpServers"], json!([]));
    assert!(sn["msg"]["params"].get("_meta").is_none());

    l.send(json!({"jsonrpc": "2.0", "id": 2, "method": "session/prompt", "params": {"sessionId": "s1", "prompt": [{"type": "text", "text": "hi"}]}})).await;
    let mut saw_update = false;
    loop {
        let m = l.recv().await;
        let method = m["method"].as_str().unwrap_or("");
        assert!(
            !method.starts_with("fs/") && !method.starts_with("terminal/"),
            "I3 violated, Zed received: {m}"
        );
        if method == "session/request_permission" {
            l.send(json!({"jsonrpc": "2.0", "id": m["id"], "result": {"outcome": {"outcome": "cancelled"}}})).await;
        } else if method == "session/update" {
            saw_update = true;
        } else if m["id"] == 2 {
            assert_eq!(m["result"]["stopReason"], "end_turn");
            break;
        }
    }
    assert!(saw_update);
    // The agent got error replies (from the relay) for its fs/terminal requests.
    let responses: Vec<Value> = env
        .agent_received()
        .into_iter()
        .filter(|m| m["kind"] == "response")
        .collect();
    for id in ["a1", "a2"] {
        let r = responses
            .iter()
            .find(|m| m["msg"]["id"] == id)
            .unwrap_or_else(|| panic!("no reply for {id}"));
        assert_eq!(r["msg"]["error"]["code"], -32601);
    }
    let a3 = responses.iter().find(|m| m["msg"]["id"] == "a3").unwrap();
    assert_eq!(a3["msg"]["result"]["outcome"]["outcome"], "cancelled");

    l.close_stdin();
    let (code, _) = l.wait(S(5)).await;
    assert_eq!(code, Some(0));
}

#[tokio::test]
async fn session_with_wrong_cwd_rejected_before_agent() {
    let env = TestEnv::new();
    let mut l = env.launch("normal");
    l.initialize().await;
    l.send(session_new(1, "/etc")).await;
    let r = l.recv().await;
    assert_eq!(r["id"], 1);
    assert_eq!(r["error"]["code"], -32602);
    assert!(r["error"]["message"]
        .as_str()
        .unwrap()
        .contains("open /etc in its own Zed window"));
    assert!(!received_methods(&env).contains(&"session/new".to_string()));
    l.close_stdin();
    l.wait(S(5)).await;
}

#[tokio::test]
async fn launch_args_recorded_are_exact() {
    let env = TestEnv::new();
    let mut l = env.launch("normal");
    l.initialize().await;
    let calls = env.container_calls();
    let run = calls
        .iter()
        .find(|c| c.starts_with("run "))
        .expect("no run call");
    let p = env.project.to_string_lossy();
    let state = env.state_root.join("state/claude");
    let s = state.to_string_lossy();
    assert!(
        run.starts_with("run -i --rm --init --progress none --name zevbox-claude-"),
        "{run}"
    );
    assert!(
        run.contains(" --label zevbox.managed=1 --label zevbox.instance="),
        "{run}"
    );
    assert!(run.contains(" --label zevbox.agent=claude --cpus 2 --memory 1G --uid 501 --gid 20 --cap-drop ALL -e HOME=/home/agent "), "{run}");
    assert!(run.contains(&format!(" --mount type=bind,source={p},target={p} --mount type=bind,source={s},target=/home/agent -w {p} ")), "{run}");
    assert!(
        run.ends_with(
            " zevbox/claude:claude-test /opt/zevbox/claude/node_modules/.bin/claude-agent-acp"
        ),
        "{run}"
    );
    l.close_stdin();
    l.wait(S(5)).await;
}

#[tokio::test]
async fn instance_record_exists_while_running() {
    let env = TestEnv::new();
    let mut l = env.launch("normal");
    l.initialize().await;
    let recs = env.instance_records();
    assert_eq!(recs.len(), 1);
    let rec: Value = serde_json::from_str(&std::fs::read_to_string(&recs[0]).unwrap()).unwrap();
    assert_eq!(rec["agent"], "claude");
    assert_eq!(rec["release"], "claude-test");
    assert_eq!(rec["image"], "zevbox/claude:claude-test");
    assert_eq!(rec["relayPid"], l.pid());
    let name = rec["containerName"].as_str().unwrap();
    assert_eq!(env.zevbox_containers(), vec![name.to_string()]);
    let id = rec["id"].as_str().unwrap();
    assert_eq!(name, format!("zevbox-claude-{id}"));

    let st = env.run(&["status", "--json"]).await;
    assert_eq!(st.code, Some(0), "{}", st.stderr);
    let v: Value = serde_json::from_str(&st.stdout).unwrap();
    assert_eq!(v[0]["id"], id);
    assert_eq!(v[0]["state"], "running");
    assert_eq!(v[0]["agent"], "claude");
    assert_eq!(v[0]["release"], "claude-test");
    assert_eq!(v[0]["containerName"], name);
    l.close_stdin();
    l.wait(S(5)).await;
}

// ---------- shutdown paths (§8.3) ----------

#[tokio::test]
async fn eof_stops_container_within_bound() {
    let env = TestEnv::new();
    let mut l = env.launch("normal");
    l.initialize().await;
    l.close_stdin();
    let (code, elapsed) = l.wait(S(6)).await;
    assert_eq!(code, Some(0));
    assert!(elapsed < S(4), "EOF shutdown took {elapsed:?}");
    assert!(env.zevbox_containers().is_empty());
    assert!(env.instance_records().is_empty());
}

#[tokio::test]
async fn stubborn_agent_gets_stop_then_delete() {
    let env = TestEnv::new();
    let mut l = env.launch("stubborn");
    l.initialize().await;
    l.close_stdin();
    let (_, elapsed) = l.wait(S(15)).await;
    assert!(elapsed < S(13), "took {elapsed:?}");
    assert!(env
        .container_calls()
        .iter()
        .any(|c| c.starts_with("stop ") && c.contains("zevbox-claude-")));
    assert!(env.zevbox_containers().is_empty());
    assert!(env.instance_records().is_empty());
}

#[tokio::test]
async fn sigterm_triggers_cleanup() {
    let env = TestEnv::new();
    let mut l = env.launch("stubborn");
    l.initialize().await;
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(l.pid()),
        nix::sys::signal::Signal::SIGTERM,
    )
    .unwrap();
    let (_, elapsed) = l.wait(S(15)).await;
    assert!(elapsed < S(13), "took {elapsed:?}");
    assert!(env.zevbox_containers().is_empty());
    assert!(env.instance_records().is_empty());
}

#[tokio::test]
async fn agent_crash_cleans_up() {
    let env = TestEnv::new();
    let mut l = env.launch("crash");
    l.initialize().await;
    l.wait(S(8)).await;
    assert!(env.zevbox_containers().is_empty());
    assert!(env.instance_records().is_empty());
}

#[tokio::test]
async fn invalid_agent_output_is_fatal_and_cleans_up() {
    let env = TestEnv::new();
    let mut l = env.launch("garbage");
    l.initialize().await;
    // The garbage line must never reach Zed: recv() would fail on non-JSON.
    assert!(l.try_recv(S(3)).await.is_none());
    let (code, _) = l.wait(S(15)).await;
    assert_ne!(code, Some(0), "a protocol violation is an error exit");
    assert!(env.zevbox_containers().is_empty());
    assert!(env.instance_records().is_empty());
}

// ---------- preflight errors reach Zed (§8.5) ----------

async fn expect_preflight_error(env: &TestEnv, project: &str, needle: &str) {
    let mut l = env.launch_with("normal", project);
    let r = l.initialize().await;
    assert_eq!(r["id"], 0);
    assert_eq!(r["error"]["code"], -32000, "{r}");
    let msg = r["error"]["message"].as_str().unwrap();
    assert!(msg.starts_with("zevbox: "), "{msg}");
    assert!(
        msg.contains(needle),
        "message should contain {needle:?}: {msg}"
    );
    let (code, _) = l.wait(S(5)).await;
    assert_eq!(code, Some(3));
    assert!(
        !env.container_calls().iter().any(|c| c.starts_with("run ")),
        "no container may start"
    );
}

/// I2 zones (DECISIONS E1): a hidden home folder is refused before any container.
#[tokio::test]
async fn preflight_forbidden_project() {
    let env = TestEnv::new();
    let hidden = env.home.join(".config/tool");
    std::fs::create_dir_all(&hidden).unwrap();
    expect_preflight_error(&env, &hidden.to_string_lossy(), "is not an allowed project").await;
}

#[tokio::test]
async fn preflight_missing_auth() {
    let env = TestEnv::new();
    std::fs::remove_file(
        env.state_root
            .join("state/claude/.claude/.credentials.json"),
    )
    .unwrap();
    expect_preflight_error(&env, &env.project.to_string_lossy(), "zevbox auth claude").await;
}

#[tokio::test]
async fn preflight_no_active_release() {
    let env = TestEnv::new();
    std::fs::remove_file(env.state_root.join("releases/active.json")).unwrap();
    expect_preflight_error(&env, &env.project.to_string_lossy(), "zevbox update").await;
}

#[tokio::test]
async fn preflight_missing_image() {
    let env = TestEnv::new();
    // `container image list` returns no image at all.
    let mut l = env.launch_with_env(
        "normal",
        &env.project.to_string_lossy(),
        &[("FAKE_IMAGES", "")],
    );
    let r = l.initialize().await;
    assert_eq!(r["error"]["code"], -32000, "{r}");
    assert!(r["error"]["message"]
        .as_str()
        .unwrap()
        .contains("zevbox update"));
    assert_eq!(l.wait(S(5)).await.0, Some(3));
    assert!(!env.container_calls().iter().any(|c| c.starts_with("run ")));
}

#[tokio::test]
async fn preflight_missing_config() {
    let env = TestEnv::bare();
    expect_preflight_error(&env, &env.project.to_string_lossy(), "zevbox setup").await;
}

// ---------- targeting and orphans (I8, §8.4) ----------

#[tokio::test]
async fn two_launches_get_distinct_names() {
    let env = TestEnv::new();
    let mut a = env.launch("normal");
    let mut b = env.launch("normal");
    a.initialize().await;
    b.initialize().await;
    let names = env.zevbox_containers();
    assert_eq!(names.len(), 2);
    assert_ne!(names[0], names[1]);
    a.close_stdin();
    b.close_stdin();
    a.wait(S(5)).await;
    b.wait(S(5)).await;
    assert!(env.zevbox_containers().is_empty());
}

#[tokio::test]
async fn stop_without_args_stops_only_zevbox_instances() {
    let env = TestEnv::new();
    let mut foreign = env.add_foreign_container("buildkit");
    let mut l = env.launch("stubborn");
    l.initialize().await;
    let out = env.run(&["stop"]).await;
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    l.wait(S(15)).await;
    assert!(env.zevbox_containers().is_empty());
    assert!(
        env.containers()
            .iter()
            .any(|(n, s)| n == "buildkit" && s == "running"),
        "foreign container touched"
    );
    for c in env.container_calls() {
        assert!(!c.contains("buildkit"), "foreign container targeted: {c}");
    }
    foreign.kill().ok();
}

#[tokio::test]
async fn stop_by_id_and_unknown_id() {
    let env = TestEnv::new();
    let mut l = env.launch("stubborn");
    l.initialize().await;
    let bad = env.run(&["stop", "ffffffff"]).await;
    assert_eq!(bad.code, Some(1));
    let rec: Value =
        serde_json::from_str(&std::fs::read_to_string(&env.instance_records()[0]).unwrap())
            .unwrap();
    let ok = env.run(&["stop", rec["id"].as_str().unwrap()]).await;
    assert_eq!(ok.code, Some(0), "{}", ok.stderr);
    l.wait(S(15)).await;
    assert!(env.zevbox_containers().is_empty());
}

#[tokio::test]
async fn orphan_record_detected_and_cleaned() {
    let env = TestEnv::new();
    let mut dead = std::process::Command::new("/usr/bin/true").spawn().unwrap();
    let dead_pid = dead.id();
    dead.wait().unwrap();
    let mut sleeper = env.add_unregistered_zevbox_container("zevbox-claude-deadbeef");
    common::write_json(
        &env.state_root.join("instances/deadbeef.json"),
        &json!({
            "id": "deadbeef", "agent": "claude", "project": env.project, "release": "claude-test",
            "image": "zevbox/claude:claude-test", "containerName": "zevbox-claude-deadbeef",
            "relayPid": dead_pid, "relayStartedAt": "Mon Jan  1 00:00:00 2024", "createdAt": "2024-01-01T00:00:00Z"
        }),
    );
    let st = env.run(&["status", "--json"]).await;
    let v: Value = serde_json::from_str(&st.stdout).unwrap();
    let row = v
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "deadbeef")
        .expect("orphan not listed");
    assert_eq!(row["state"], "orphan");
    assert_eq!(env.instance_records().len(), 1, "status is read-only");

    let out = env.run(&["clean"]).await;
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    assert!(env.instance_records().is_empty());
    assert!(env.zevbox_containers().is_empty());
    sleeper.kill().ok();
}

#[tokio::test]
async fn unregistered_container_listed_and_cleaned_but_foreign_kept() {
    let env = TestEnv::new();
    let mut foreign = env.add_foreign_container("kiac-forgejo-agentic-worker-1");
    let mut z = env.add_unregistered_zevbox_container("zevbox-claude-cafebabe");
    let st = env.run(&["status", "--json"]).await;
    let v: Value = serde_json::from_str(&st.stdout).unwrap();
    let row = v
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["containerName"] == "zevbox-claude-cafebabe")
        .unwrap();
    assert_eq!(row["state"], "unregistered");
    assert!(v
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["containerName"] != "kiac-forgejo-agentic-worker-1"));

    assert_eq!(env.run(&["clean"]).await.code, Some(0));
    assert!(env.zevbox_containers().is_empty());
    assert!(env
        .containers()
        .iter()
        .any(|(n, _)| n == "kiac-forgejo-agentic-worker-1"));
    z.kill().ok();
    foreign.kill().ok();
}

#[tokio::test]
async fn launch_cleans_orphans_first() {
    let env = TestEnv::new();
    let mut dead = std::process::Command::new("/usr/bin/true").spawn().unwrap();
    let dead_pid = dead.id();
    dead.wait().unwrap();
    common::write_json(
        &env.state_root.join("instances/0badf00d.json"),
        &json!({
            "id": "0badf00d", "agent": "claude", "project": env.project, "release": "claude-test",
            "image": "zevbox/claude:claude-test", "containerName": "zevbox-claude-0badf00d",
            "relayPid": dead_pid, "relayStartedAt": "x", "createdAt": "2024-01-01T00:00:00Z"
        }),
    );
    let mut l = env.launch("normal");
    l.initialize().await;
    assert!(
        eventually(S(3), || !env
            .state_root
            .join("instances/0badf00d.json")
            .exists())
        .await
    );
    l.close_stdin();
    l.wait(S(5)).await;
}

#[tokio::test]
async fn never_uses_all_flag_or_tty_for_acp() {
    let env = TestEnv::new();
    let mut l = env.launch("stubborn");
    l.initialize().await;
    env.run(&["status"]).await;
    env.run(&["stop"]).await;
    l.wait(S(15)).await;
    env.run(&["clean"]).await;
    let calls = env.container_calls();
    assert!(
        calls.iter().any(|c| c.starts_with("stop ")),
        "stop was expected: {calls:?}"
    );
    for c in &calls {
        let words: Vec<&str> = c.split(' ').collect();
        // `list --all` is the only legitimate use of --all (it only reads).
        if words[0] != "list" {
            assert!(
                !words.contains(&"--all") && !words.contains(&"-a"),
                "I8: forbidden --all in: {c}"
            );
        }
        if words[0] == "run" {
            for bad in ["-t", "-it", "--tty"] {
                assert!(!words.contains(&bad), "no TTY for ACP: {c}");
            }
        }
    }
}

//! §8.1 — End-to-end tests of `init`, `project`, `zed`, `doctor` exit codes.
//! PROTECTED FILE: do not edit.
//! 2026-10-14: Zed entries are one per agent (DECISIONS R1), edited with the
//! human's authorization.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::TestEnv;
use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;

fn config(env: &TestEnv) -> Value {
    serde_json::from_str(&fs::read_to_string(env.state_root.join("config.json")).unwrap()).unwrap()
}

fn mode(p: &std::path::Path) -> u32 {
    fs::metadata(p).unwrap().permissions().mode() & 0o777
}

#[tokio::test]
async fn init_creates_private_layout_and_is_idempotent() {
    let env = TestEnv::bare();
    let out = env.run(&["init"]).await;
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    for d in [
        "",
        "state",
        "state/claude",
        "state/antigravity",
        "instances",
        "logs",
        "backups",
        "releases",
    ] {
        let p = env.state_root.join(d);
        assert!(p.is_dir(), "{d} missing");
        assert_eq!(mode(&p), 0o700, "{d} must be 0700");
    }
    let c = config(&env);
    assert_eq!(c["schemaVersion"], 1);
    assert_eq!(c["projects"], serde_json::json!([]));
    assert!(c["containerBinary"].as_str().unwrap().starts_with('/'));
    let tools: Value =
        serde_json::from_str(&fs::read_to_string(env.state_root.join("tools.json")).unwrap())
            .unwrap();
    assert_eq!(tools, serde_json::json!({"schemaVersion": 1, "apt": []}));

    // Second run never overwrites existing files.
    let mut c2 = c.clone();
    c2["resources"]["cpus"] = 7.into();
    fs::write(env.state_root.join("config.json"), c2.to_string()).unwrap();
    assert_eq!(env.run(&["init"]).await.code, Some(0));
    assert_eq!(config(&env)["resources"]["cpus"], 7);
}

#[tokio::test]
async fn project_add_list_remove() {
    let env = TestEnv::new();
    let p2 = env.home.join("code/second");
    fs::create_dir_all(&p2).unwrap();
    let link = env.home.join("second-link");
    std::os::unix::fs::symlink(&p2, &link).unwrap();

    let out = env
        .run(&["project", "add", link.to_str().unwrap(), "--no-zed"])
        .await;
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let projects = config(&env)["projects"].clone();
    assert_eq!(projects.as_array().unwrap().len(), 2);
    assert_eq!(
        projects[1],
        p2.to_string_lossy().as_ref(),
        "stored canonical"
    );

    // Idempotent.
    assert_eq!(
        env.run(&["project", "add", p2.to_str().unwrap(), "--no-zed"])
            .await
            .code,
        Some(0)
    );
    assert_eq!(config(&env)["projects"].as_array().unwrap().len(), 2);

    let list = env.run(&["project", "list"]).await;
    assert!(list.stdout.contains(p2.to_str().unwrap()));
    assert!(list.stdout.contains(env.project.to_str().unwrap()));

    assert_eq!(
        env.run(&["project", "remove", p2.to_str().unwrap()])
            .await
            .code,
        Some(0)
    );
    assert_eq!(config(&env)["projects"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn project_add_rejects_dangerous_paths() {
    let env = TestEnv::new();
    for bad in [
        env.home.clone(),
        std::path::PathBuf::from("/"),
        env.state_root.clone(),
        env.home.join("does-not-exist"),
    ] {
        let out = env
            .run(&["project", "add", bad.to_str().unwrap(), "--no-zed"])
            .await;
        assert_eq!(out.code, Some(1), "{} must be refused", bad.display());
        assert!(
            !out.stderr.is_empty(),
            "an explanation is required on stderr"
        );
    }
    assert_eq!(config(&env)["projects"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn zed_install_dry_run_install_uninstall() {
    let env = TestEnv::new();
    let fixture =
        fs::read_to_string(common::manifest_dir().join("tests/fixtures/zed/with_comments.json"))
            .unwrap();
    fs::write(&env.zed_settings, &fixture).unwrap();
    fs::set_permissions(&env.zed_settings, fs::Permissions::from_mode(0o600)).unwrap();

    let dry = env.run(&["zed", "install", "--dry-run"]).await;
    assert_eq!(dry.code, Some(0), "{}", dry.stderr);
    assert_eq!(
        fs::read_to_string(&env.zed_settings).unwrap(),
        fixture,
        "dry-run must not write"
    );

    let out = env.run(&["zed", "install"]).await;
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let text = fs::read_to_string(&env.zed_settings).unwrap();
    let v: Value = jsonc_parser::parse_to_serde_value(&text, &Default::default()).unwrap();
    let servers = v["agent_servers"].as_object().unwrap();
    // Only Claude has an active release in TestEnv::new().
    let key = "zevbox · Claude";
    assert!(servers.contains_key(key), "{text}");
    assert!(
        !servers.contains_key("zevbox · Antigravity"),
        "no entry without an active release"
    );
    assert!(servers.contains_key("claude-acp"));
    assert!(!servers.contains_key("zevbox · Claude · old"));
    assert_eq!(
        servers[key]["command"],
        env.installed_binary.to_string_lossy().as_ref()
    );
    assert_eq!(
        servers[key]["args"],
        serde_json::json!(["launch", "claude"])
    );
    assert_eq!(mode(&env.zed_settings), 0o600);
    assert_eq!(
        fs::read_dir(env.state_root.join("backups"))
            .unwrap()
            .count(),
        1
    );

    let again = env.run(&["zed", "install"]).await;
    assert_eq!(again.code, Some(0));
    assert_eq!(fs::read_to_string(&env.zed_settings).unwrap(), text);
    assert_eq!(
        fs::read_dir(env.state_root.join("backups"))
            .unwrap()
            .count(),
        1
    );

    assert_eq!(env.run(&["zed", "uninstall"]).await.code, Some(0));
    let after = fs::read_to_string(&env.zed_settings).unwrap();
    assert!(!after.contains("zevbox · "));
    assert!(after.contains("// Registry agent managed by Zed itself"));
}

#[tokio::test]
async fn project_add_updates_zed_by_default() {
    let env = TestEnv::new();
    fs::write(&env.zed_settings, "{}\n").unwrap();
    let p2 = env.home.join("code/second");
    fs::create_dir_all(&p2).unwrap();
    let out = env.run(&["project", "add", p2.to_str().unwrap()]).await;
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let text = fs::read_to_string(&env.zed_settings).unwrap();
    // One entry per agent: adding a project never adds a Zed entry.
    assert!(text.contains("\"zevbox · Claude\""), "{text}");
    assert!(!text.contains("zevbox · Claude · second"), "{text}");
    assert!(!text.contains("--project"), "{text}");
    assert_eq!(
        env.run(&["project", "list"]).await.stdout.lines().count(),
        2
    );
}

#[tokio::test]
async fn commands_without_setup_explain_what_to_do() {
    let env = TestEnv::bare();
    let out = env.run(&["status"]).await;
    assert_eq!(
        out.code,
        Some(3),
        "missing config is a precondition failure"
    );
    assert!(
        out.stderr.contains("zevbox setup") || out.stderr.contains("zevbox init"),
        "{}",
        out.stderr
    );
}

#[tokio::test]
async fn status_json_is_empty_array_when_idle() {
    let env = TestEnv::new();
    let out = env.run(&["status", "--json"]).await;
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    assert_eq!(
        serde_json::from_str::<Value>(&out.stdout).unwrap(),
        serde_json::json!([])
    );
}

#[tokio::test]
async fn usage_errors_exit_2() {
    let env = TestEnv::new();
    assert_eq!(
        env.run(&["launch", "nope", "--project", "/x"]).await.code,
        Some(2)
    );
    assert_eq!(env.run(&["frobnicate"]).await.code, Some(2));
}

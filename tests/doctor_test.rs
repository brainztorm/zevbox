//! `zevbox doctor` (§8.6): agent lines keep release/auth, flag a missing image or a
//! missing sign-in, and the exit code follows FAIL. Not in the protected manifest.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::TestEnv;
use serde_json::Value;

async fn checks(env: &TestEnv) -> (Option<i32>, Vec<Value>) {
    let out = env.run(&["doctor", "--json"]).await;
    let parsed: Vec<Value> = serde_json::from_str(&out.stdout).unwrap_or_default();
    (out.code, parsed)
}

fn find<'a>(checks: &'a [Value], name: &str) -> &'a Value {
    checks
        .iter()
        .find(|c| c["check"] == name)
        .unwrap_or_else(|| panic!("no check {name}: {checks:?}"))
}

#[tokio::test]
async fn signed_out_agent_is_a_warning_that_keeps_release_and_fix() {
    let env = TestEnv::new();
    std::fs::remove_file(
        env.state_root
            .join("state/claude/.claude/.credentials.json"),
    )
    .unwrap();
    let (code, c) = checks(&env).await;
    let agent = find(&c, "agent:claude");
    assert_eq!(agent["status"], "WARN", "{agent}");
    let msg = agent["message"].as_str().unwrap();
    assert!(msg.contains("not authenticated"), "{msg}");
    assert!(msg.contains(common::RELEASE), "{msg}");
    assert!(msg.contains("zevbox auth claude"), "{msg}");
    assert_eq!(code, Some(0), "a warning is not a failure");
}

#[tokio::test]
async fn missing_image_is_a_failure() {
    let env = TestEnv::new();
    let lock_path = env
        .state_root
        .join(format!("releases/{}/lock.json", common::RELEASE));
    let mut lock: Value =
        serde_json::from_str(&std::fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["image"] = "zevbox/claude:not-built".into();
    common::write_json(&lock_path, &lock);
    let (code, c) = checks(&env).await;
    let agent = find(&c, "agent:claude");
    assert_eq!(agent["status"], "FAIL", "{agent}");
    assert!(agent["message"]
        .as_str()
        .unwrap()
        .contains("zevbox update --agent claude"));
    assert_eq!(code, Some(1));
}

#[tokio::test]
async fn permissions_and_orphans_are_reported() {
    use std::os::unix::fs::PermissionsExt;
    let env = TestEnv::new();
    std::fs::set_permissions(
        env.state_root.join("state/claude"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let (_, c) = checks(&env).await;
    let perms = find(&c, "permissions");
    assert_eq!(perms["status"], "WARN", "{perms}");
    assert!(
        perms["message"].as_str().unwrap().contains("state/claude"),
        "{perms}"
    );
    assert_eq!(find(&c, "instances")["status"], "OK");
}

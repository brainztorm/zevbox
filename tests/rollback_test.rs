//! `zevbox rollback` (§8.1): refuses when the image of `previous` is gone.
//! Not in the protected manifest.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::TestEnv;
use serde_json::{json, Value};

fn active(env: &TestEnv) -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(env.state_root.join("releases/active.json")).unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn rollback_refuses_when_previous_image_is_gone() {
    let env = TestEnv::new();
    let before = json!({"claude": {"current": common::RELEASE, "previous": "claude-gone"}});
    common::write_json(&env.state_root.join("releases/active.json"), &before);
    let out = env.run(&["rollback", "--agent", "claude"]).await;
    assert_eq!(out.code, Some(1), "{}", out.stderr);
    assert!(out.stderr.contains("no longer exists"), "{}", out.stderr);
    assert!(
        out.stderr.contains("zevbox update --agent claude"),
        "{}",
        out.stderr
    );
    assert_eq!(active(&env), before, "active.json must be untouched");
}

#[tokio::test]
async fn rollback_swaps_when_previous_image_exists() {
    let env = TestEnv::new();
    // `claude-test` (the fake image `zevbox/claude:claude-test`) becomes `previous`.
    common::write_json(
        &env.state_root.join("releases/active.json"),
        &json!({"claude": {"current": "claude-newer", "previous": common::RELEASE}}),
    );
    let out = env.run(&["rollback", "--agent", "claude"]).await;
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    assert_eq!(active(&env)["claude"]["current"], common::RELEASE);
}

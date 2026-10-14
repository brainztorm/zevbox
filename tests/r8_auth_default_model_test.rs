//! DECISIONS R8 — `zevbox auth claude` writes a default model in the
//! container's Claude settings. Not in the protected manifest.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::TestEnv;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use zevbox::cli::auth::ensure_claude_default_model;
use zevbox::paths::Paths;

fn settings(env: &TestEnv) -> std::path::PathBuf {
    env.state_root.join("state/claude/.claude/settings.json")
}

#[tokio::test]
async fn status_does_not_create_the_file() {
    let env = TestEnv::new();
    let out = env.run(&["auth", "claude", "--status"]).await;
    assert_eq!(out.code, Some(0));
    assert!(!settings(&env).exists());
}

#[test]
fn writes_fable_without_host_settings() {
    let env = TestEnv::new();
    let paths = Paths::new(&env.state_root);
    let got = ensure_claude_default_model(&paths, &env.home.join("missing.json")).unwrap();
    assert_eq!(got.as_deref(), Some("fable"));
    let text = fs::read_to_string(settings(&env)).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&text).unwrap(),
        serde_json::json!({"model": "fable"})
    );
    let mode = fs::metadata(settings(&env)).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn copies_only_the_host_model() {
    let env = TestEnv::new();
    let paths = Paths::new(&env.state_root);
    let host = env.home.join("host.json");
    fs::write(&host, r#"{"model": "haiku", "env": {"X": "secret"}}"#).unwrap();
    let got = ensure_claude_default_model(&paths, &host).unwrap();
    assert_eq!(got.as_deref(), Some("haiku"));
    let text = fs::read_to_string(settings(&env)).unwrap();
    assert!(!text.contains("secret"));
}

#[test]
fn existing_file_is_left_alone() {
    let env = TestEnv::new();
    let paths = Paths::new(&env.state_root);
    fs::write(settings(&env), "{}").unwrap();
    assert_eq!(
        ensure_claude_default_model(&paths, &env.home.join("none.json")).unwrap(),
        None
    );
    assert_eq!(fs::read_to_string(settings(&env)).unwrap(), "{}");
}

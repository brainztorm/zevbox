//! DECISIONS R2 — `auth` works without `config.json` for `--status`, and does
//! not rebuild anything when everything is in place. The "init then update"
//! path needs `container build` and the network: tested by hand.
//! Not in the protected manifest.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::TestEnv;

#[tokio::test]
async fn status_without_config_is_unknown_and_creates_nothing() {
    let env = TestEnv::bare();
    let out = env.run(&["auth", "claude", "--status"]).await;
    assert_eq!(out.code, Some(0));
    assert_eq!(out.stdout.trim(), "unknown");
    assert!(!env.state_root.join("config.json").exists());
}

#[tokio::test]
async fn status_with_release_does_not_build() {
    let env = TestEnv::new();
    let out = env.run(&["auth", "claude", "--status"]).await;
    assert_eq!(out.code, Some(0));
    assert_eq!(out.stdout.trim(), "authenticated");
    assert!(!env.container_calls().iter().any(|c| c.contains("build")));
}

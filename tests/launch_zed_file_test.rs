//! A project whose `.zed` is a plain file is refused at launch with a message Zed
//! shows (I13), before any container starts. Not in the protected manifest.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::TestEnv;
use std::time::Duration;

#[tokio::test]
async fn zed_file_in_project_is_refused_before_the_container() {
    let env = TestEnv::new();
    std::fs::write(env.project.join(".zed"), "not a folder").unwrap();
    let mut l = env.launch("normal");
    let r = l.initialize().await;
    assert_eq!(r["error"]["code"], -32000, "{r}");
    let msg = r["error"]["message"].as_str().unwrap();
    assert!(msg.contains("is a file or a link"), "{msg}");
    assert!(msg.contains("Fix:"), "{msg}");
    let (code, _) = l.wait(Duration::from_secs(5)).await;
    assert_eq!(code, Some(3));
    assert!(!env.container_calls().iter().any(|c| c.starts_with("run ")));
}

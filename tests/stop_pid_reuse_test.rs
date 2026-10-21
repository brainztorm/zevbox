//! `zevbox stop` must never signal a process that merely reuses an orphan
//! record's `relayPid` (typical after a reboot). Not in the protected manifest.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::TestEnv;
use serde_json::json;

#[tokio::test]
async fn stop_does_not_signal_a_process_that_reuses_the_pid() {
    let env = TestEnv::new();
    // An unrelated process of the user that happens to own the recorded pid.
    let mut innocent = std::process::Command::new("/bin/sleep")
        .arg("60")
        .spawn()
        .unwrap();
    common::write_json(
        &env.state_root.join("instances/0badf00d.json"),
        &json!({
            "id": "0badf00d", "agent": "claude", "project": env.project, "release": "claude-test",
            "image": "zevbox/claude:claude-test", "containerName": "zevbox-claude-0badf00d",
            // Start time differs from the real one: this record is an orphan.
            "relayPid": innocent.id(), "relayStartedAt": "Thu Jan  1 00:00:00 1970",
            "createdAt": "2024-01-01T00:00:00Z"
        }),
    );
    let out = env.run(&["stop"]).await;
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    assert!(
        innocent.try_wait().unwrap().is_none(),
        "the unrelated process must still be running"
    );
    assert!(
        out.stdout.contains("stopped 0badf00d"),
        "stop must say what it stopped: {}",
        out.stdout
    );
    assert!(!env.state_root.join("instances/0badf00d.json").exists());
    innocent.kill().ok();
    innocent.wait().ok();
}

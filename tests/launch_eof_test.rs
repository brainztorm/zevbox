//! A failed preflight with stdin closed must exit, not spin (regression: the
//! wait-for-Zed's-first-request loop treated end of input as an empty line).
//! Not in the protected manifest.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::TestEnv;
use std::time::Duration;

#[tokio::test]
async fn failed_preflight_exits_when_stdin_closes() {
    let env = TestEnv::new();
    // "/etc" is outside the allowed zones: preflight fails before any container.
    let mut l = env.launch_with("normal", "/etc");
    l.close_stdin();
    let (code, _) = l.wait(Duration::from_secs(5)).await;
    assert_eq!(code, Some(3), "must exit 3 within 5 s, not loop on EOF");
}

//! `zevbox zed install --dry-run` shows what it would write, then writes nothing
//! (plan §8.1). Not in the protected manifest.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::TestEnv;

#[tokio::test]
async fn dry_run_lists_entries_and_install_reports_backup() {
    let env = TestEnv::new();
    std::fs::write(&env.zed_settings, "{}\n").unwrap();
    let dry = env.run(&["zed", "install", "--dry-run"]).await;
    assert_eq!(dry.code, Some(0), "{}", dry.stderr);
    assert!(
        dry.stdout.contains("would write 1 managed entry"),
        "{}",
        dry.stdout
    );
    assert!(dry.stdout.contains("zevbox · Claude -> "), "{}", dry.stdout);
    assert_eq!(std::fs::read_to_string(&env.zed_settings).unwrap(), "{}\n");

    let real = env.run(&["zed", "install"]).await;
    assert!(
        real.stdout.contains("zed settings: updated (backup: "),
        "{}",
        real.stdout
    );
    let again = env.run(&["zed", "install"]).await;
    assert!(
        again.stdout.contains("already up to date"),
        "{}",
        again.stdout
    );
    let dry2 = env.run(&["zed", "install", "--dry-run"]).await;
    assert!(
        dry2.stdout.contains("already up to date"),
        "{}",
        dry2.stdout
    );
}

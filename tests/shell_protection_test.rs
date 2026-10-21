//! `zevbox shell` gets the same project-file protections as the agent (I13, DECISIONS R2):
//! `.zed` masked, an existing `.git` read-only. Not in the protected manifest.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::TestEnv;

#[tokio::test]
async fn shell_masks_zed_and_locks_git() {
    let env = TestEnv::new();
    std::fs::create_dir_all(env.project.join(".git")).unwrap();
    let out = env
        .run(&[
            "shell",
            "claude",
            "--project",
            env.project.to_str().unwrap(),
        ])
        .await;
    let calls = env.container_calls();
    let run = calls
        .iter()
        .find(|c| c.starts_with("run "))
        .unwrap_or_else(|| panic!("no run call: {calls:?} / {}", out.stderr));
    let p = std::fs::canonicalize(&env.project).unwrap();
    let p = p.to_string_lossy();
    assert!(run.contains(&format!(" --tmpfs {p}/.zed ")), "{run}");
    assert!(
        run.contains(&format!(" --read-only-path {p}/.git ")),
        "{run}"
    );
    assert!(run.contains(" -it "), "{run}");
}

#[tokio::test]
async fn shell_without_git_only_masks_zed() {
    let env = TestEnv::new();
    let out = env
        .run(&[
            "shell",
            "claude",
            "--project",
            env.project.to_str().unwrap(),
        ])
        .await;
    let calls = env.container_calls();
    let run = calls.iter().find(|c| c.starts_with("run ")).unwrap();
    assert!(run.contains("--tmpfs "), "{run} / {}", out.stderr);
    assert!(!run.contains("--read-only-path"), "{run}");
}

#[tokio::test]
async fn shell_refuses_a_zed_file() {
    let env = TestEnv::new();
    std::fs::write(env.project.join(".zed"), "x").unwrap();
    let out = env
        .run(&[
            "shell",
            "claude",
            "--project",
            env.project.to_str().unwrap(),
        ])
        .await;
    assert_eq!(out.code, Some(3), "{}", out.stderr);
    assert!(out.stderr.contains("is a file or a link"), "{}", out.stderr);
    assert!(!env.container_calls().iter().any(|c| c.starts_with("run ")));
}

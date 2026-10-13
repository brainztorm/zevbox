//! §8.2 / I8 — `container` argument building and JSON parsing. PROTECTED FILE: do not edit.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use zevbox::container::{
    image_matches, is_zevbox_name, may_target, new_instance_id, parse_image_list_json,
    parse_list_json, parse_system_status_json, run_args, ContainerCli, ContainerError,
    ContainerInfo, RunSpec,
};

fn fixture(name: &str) -> String {
    fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/container")
            .join(name),
    )
    .unwrap()
}

fn spec(interactive: bool) -> RunSpec {
    RunSpec {
        name: "zevbox-claude-0a1b2c3d".into(),
        instance_id: "0a1b2c3d".into(),
        agent: "claude".into(),
        cpus: 4,
        memory: "4G".into(),
        uid: 501,
        gid: 20,
        project: PathBuf::from("/Users/test/code/my project"),
        state_dir: PathBuf::from("/Users/test/Library/Application Support/zevbox/state/claude"),
        image: "zevbox/claude:claude-20261013-101500".into(),
        command: vec!["/opt/zevbox/claude/node_modules/.bin/claude-agent-acp".into()],
        interactive,
    }
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

#[test]
fn launch_args_exact() {
    assert_eq!(
        run_args(&spec(false)),
        s(&[
            "run", "-i", "--rm", "--init",
            "--progress",
            "none",
            "--name", "zevbox-claude-0a1b2c3d",
            "--label", "zevbox.managed=1",
            "--label", "zevbox.instance=0a1b2c3d",
            "--label", "zevbox.agent=claude",
            "--cpus", "4",
            "--memory", "4G",
            "--uid", "501",
            "--gid", "20",
            "--cap-drop", "ALL",
            "-e", "HOME=/home/agent",
            "--mount", "type=bind,source=/Users/test/code/my project,target=/Users/test/code/my project",
            "--mount", "type=bind,source=/Users/test/Library/Application Support/zevbox/state/claude,target=/home/agent",
            "-w", "/Users/test/code/my project",
            "zevbox/claude:claude-20261013-101500",
            "/opt/zevbox/claude/node_modules/.bin/claude-agent-acp",
        ])
    );
}

#[test]
fn interactive_args_use_tty_and_label() {
    let mut sp = spec(true);
    sp.name = "zevbox-claude-sh-0a1b2c3d".into();
    sp.command = vec!["bash".into()];
    let a = run_args(&sp);
    assert_eq!(
        &a[..6],
        &s(&["run", "-it", "--rm", "--init", "--progress", "none"])[..]
    );
    assert!(a
        .windows(2)
        .any(|w| w == ["--label", "zevbox.interactive=1"]));
    assert_eq!(a.last().unwrap(), "bash");
}

#[test]
fn acp_args_never_contain_forbidden_flags() {
    let a = run_args(&spec(false));
    for bad in [
        "-t",
        "-it",
        "--tty",
        "-p",
        "--publish",
        "--publish-socket",
        "--ssh",
        "--virtualization",
        "-v",
        "--volume",
        "--all",
        "-a",
        "-d",
        "--detach",
    ] {
        assert!(!a.iter().any(|x| x == bad), "forbidden flag {bad} in {a:?}");
    }
    assert_eq!(
        a.iter().filter(|x| *x == "--mount").count(),
        2,
        "I1: exactly two mounts"
    );
    assert!(!a.iter().any(|x| x == "-e") || a.iter().filter(|x| *x == "-e").count() == 1);
}

#[test]
fn parses_container_list() {
    let list = parse_list_json(&fixture("list.json")).unwrap();
    assert_eq!(list.len(), 4);
    assert_eq!(list[0].name, "buildkit");
    assert_eq!(list[0].state, "running");
    assert_eq!(
        list[1].labels.get("zevbox.managed").map(String::as_str),
        Some("1")
    );
    assert!(list[2].labels.is_empty());
    assert_eq!(list[3].state, "unknown");
}

#[test]
fn parses_empty_list_and_rejects_garbage() {
    assert!(parse_list_json("[]").unwrap().is_empty());
    assert!(matches!(
        parse_list_json("ID STATE\nbuildkit running"),
        Err(ContainerError::BadJson(_))
    ));
    assert!(matches!(
        parse_list_json("{\"a\":1}"),
        Err(ContainerError::BadJson(_))
    ));
}

#[test]
fn parses_image_list() {
    let imgs = parse_image_list_json(&fixture("image-list.json")).unwrap();
    let refs: Vec<&str> = imgs.iter().map(|i| i.reference.as_str()).collect();
    assert_eq!(
        refs,
        vec![
            "code.forgejo.org/forgejo/forgejo:16.0.2-rootless",
            "zevbox/claude:claude-20261013-101500"
        ]
    );
}

#[test]
fn parses_system_status() {
    let st = parse_system_status_json(&fixture("system-status.json")).unwrap();
    assert!(st.running);
    assert_eq!(st.version, "1.5.0");
    assert_eq!(st.architecture, "arm64");
    let stopped = parse_system_status_json(
        r#"{"status":"stopped","client":{"version":"1.5.0"},"host":{"architecture":"arm64"}}"#,
    )
    .unwrap();
    assert!(!stopped.running);
}

#[test]
fn zevbox_name_rules() {
    for ok in ["zevbox-claude-0a1b2c3d", "zevbox-antigravity-sh-deadbeef"] {
        assert!(is_zevbox_name(ok), "{ok}");
    }
    for bad in [
        "buildkit",
        "zevbox",
        "zevbox-",
        "my-zevbox-x",
        "zevbox-Claude",
        "zevbox-a b",
        "zevbox-x;rm",
        "--all",
        "-a",
    ] {
        assert!(!is_zevbox_name(bad), "{bad}");
    }
}

#[test]
fn may_target_requires_name_and_label() {
    let list = parse_list_json(&fixture("list.json")).unwrap();
    let verdicts: Vec<bool> = list.iter().map(may_target).collect();
    // buildkit: no; zevbox-claude-…: yes; no label: no; impostor: name ok + label → yes by rule.
    assert_eq!(verdicts, vec![false, true, false, true]);
    let wrong_label = ContainerInfo {
        name: "zevbox-claude-11111111".into(),
        labels: [("zevbox.managed".to_string(), "true".to_string())]
            .into_iter()
            .collect(),
        state: "running".into(),
    };
    assert!(
        !may_target(&wrong_label),
        "label value must be exactly \"1\""
    );
}

#[test]
fn instance_ids_are_8_hex_and_unique() {
    let ids: std::collections::HashSet<String> = (0..200).map(|_| new_instance_id()).collect();
    assert_eq!(ids.len(), 200);
    for id in &ids {
        assert_eq!(id.len(), 8, "{id}");
        assert!(
            id.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "{id}"
        );
    }
}

#[tokio::test]
async fn cli_refuses_non_zevbox_names_without_running_anything() {
    // The binary does not exist: if the CLI tried to run it, we would get Spawn, not Refused.
    let cli = ContainerCli::new(PathBuf::from("/nonexistent/container"));
    for name in [
        "buildkit",
        "kiac-forgejo-agentic-worker-1",
        "zevbox",
        "--all",
    ] {
        assert!(
            matches!(cli.stop(name, 3).await, Err(ContainerError::Refused(_))),
            "{name}"
        );
        assert!(
            matches!(
                cli.delete_force(name).await,
                Err(ContainerError::Refused(_))
            ),
            "{name}"
        );
    }
}

#[test]
fn image_reference_normalization() {
    // `container image list` prints fully-qualified names.
    assert!(image_matches(
        "docker.io/zevbox/claude:claude-20261013-101500",
        "zevbox/claude:claude-20261013-101500"
    ));
    assert!(image_matches(
        "zevbox/claude:claude-1",
        "zevbox/claude:claude-1"
    ));
    assert!(image_matches(
        "docker.io/library/debian:bookworm-slim",
        "debian:bookworm-slim"
    ));
    assert!(image_matches(
        "docker.io/library/debian:bookworm-slim",
        "docker.io/library/debian:bookworm-slim"
    ));
    assert!(!image_matches(
        "docker.io/zevbox/claude:claude-1",
        "zevbox/claude:claude-2"
    ));
    assert!(!image_matches(
        "ghcr.io/zevbox/claude:claude-1",
        "zevbox/claude:claude-1"
    ));
    assert!(!image_matches(
        "docker.io/evil/zevbox/claude:claude-1",
        "zevbox/claude:claude-1"
    ));
}

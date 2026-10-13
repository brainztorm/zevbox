//! §10 step 15 — Real Apple Container tests (no model, no network after the
//! base image is present). Run with `make test-integration`. PROTECTED FILE.
//!
//! They use `docker.io/library/debian:bookworm-slim` as a stand-in image: what
//! is tested is zevbox's isolation (mounts, uid, lifecycle), not the agent.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::TestEnv;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use zevbox::container::{
    image_matches, new_instance_id, parse_image_list_json, parse_list_json, run_args, RunSpec,
};

const BASE: &str = "docker.io/library/debian:bookworm-slim";

fn container_bin() -> PathBuf {
    let p = PathBuf::from(
        std::env::var("ZEVBOX_CONTAINER_BIN")
            .unwrap_or_else(|_| "/opt/homebrew/bin/container".into()),
    );
    assert!(p.is_file(), "container CLI not found at {}", p.display());
    p
}

fn ensure_base_image() {
    let out = Command::new(container_bin())
        .args(["image", "list", "--format", "json"])
        .output()
        .unwrap();
    let imgs = parse_image_list_json(&String::from_utf8_lossy(&out.stdout)).unwrap();
    if !imgs.iter().any(|i| image_matches(&i.reference, BASE)) {
        let st = Command::new(container_bin())
            .args(["image", "pull", BASE])
            .status()
            .unwrap();
        assert!(st.success(), "cannot pull {BASE}");
    }
}

/// (name, state) of every container, from the real CLI.
fn all_containers() -> Vec<(String, String)> {
    let out = Command::new(container_bin())
        .args(["list", "--all", "--format", "json"])
        .output()
        .unwrap();
    parse_list_json(&String::from_utf8_lossy(&out.stdout))
        .unwrap()
        .into_iter()
        .map(|c| (c.name, c.state))
        .collect()
}

fn foreign_snapshot() -> Vec<(String, String)> {
    let mut v: Vec<_> = all_containers()
        .into_iter()
        .filter(|(n, _)| !n.starts_with("zevbox-"))
        .collect();
    v.sort();
    v
}

/// Deletes the named container when dropped (test hygiene only).
struct Cleanup(String);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = Command::new(container_bin())
            .args(["delete", "--force", &self.0])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

struct Sandbox {
    _tmp: tempfile::TempDir,
    project: PathBuf,
    state: PathBuf,
    witness: PathBuf,
    secret: String,
}

fn sandbox() -> Sandbox {
    let tmp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    let project = root.join("code/my project");
    let state = root.join("state/claude");
    let outside = root.join("outside");
    for d in [&project, &state, &outside] {
        fs::create_dir_all(d).unwrap();
    }
    let secret = format!("secret-{}", new_instance_id());
    let witness = outside.join("witness.txt");
    fs::write(&witness, &secret).unwrap();
    Sandbox {
        _tmp: tmp,
        project,
        state,
        witness,
        secret,
    }
}

/// Runs `sh -c <script>` in a zevbox-shaped container (same args as launch).
fn run_in_box(sb: &Sandbox, script: &str) -> (bool, String) {
    ensure_base_image();
    let id = new_instance_id();
    let name = format!("zevbox-claude-{id}");
    let _c = Cleanup(name.clone());
    let spec = RunSpec {
        name,
        instance_id: id,
        agent: "claude".into(),
        cpus: 1,
        memory: "512M".into(),
        uid: nix::unistd::getuid().as_raw(),
        gid: nix::unistd::getgid().as_raw(),
        project: sb.project.clone(),
        state_dir: sb.state.clone(),
        image: BASE.into(),
        command: vec!["sh".into(), "-c".into(), script.into()],
        interactive: false,
    };
    let out = Command::new(container_bin())
        .args(run_args(&spec))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

fn sha(p: &Path) -> String {
    zevbox::fsutil::sha256_hex(&fs::read(p).unwrap())
}

#[test]
#[ignore = "needs Apple container; run with make test-integration"]
fn witness_outside_project_is_invisible_and_unchanged() {
    let sb = sandbox();
    let before = sha(&sb.witness);
    let w = sb.witness.display();
    let (_, out) = run_in_box(&sb, &format!("cat '{w}'; mkdir -p \"$(dirname '{w}')\" 2>/dev/null; echo hacked > '{w}'; ls -la /; true"));
    assert!(
        !out.contains(&sb.secret),
        "host file readable from the VM:\n{out}"
    );
    assert_eq!(sha(&sb.witness), before, "host witness modified");
}

#[test]
#[ignore = "needs Apple container; run with make test-integration"]
fn project_writes_are_visible_with_host_uid() {
    let sb = sandbox();
    let (ok, out) = run_in_box(&sb, "echo hello > \"$PWD/zevbox-it.txt\" && pwd");
    assert!(ok, "{out}");
    assert!(
        out.contains(&sb.project.to_string_lossy().to_string()),
        "cwd must be the project path: {out}"
    );
    let f = sb.project.join("zevbox-it.txt");
    assert_eq!(fs::read_to_string(&f).unwrap(), "hello\n");
    use std::os::unix::fs::MetadataExt;
    assert_eq!(
        fs::metadata(&f).unwrap().uid(),
        nix::unistd::getuid().as_raw()
    );
    fs::remove_file(&f).unwrap();
}

#[test]
#[ignore = "needs Apple container; run with make test-integration"]
fn symlink_to_outside_is_not_followed() {
    let sb = sandbox();
    std::os::unix::fs::symlink(&sb.witness, sb.project.join("link")).unwrap();
    let (_, out) = run_in_box(&sb, "cat \"$PWD/link\"; true");
    assert!(
        !out.contains(&sb.secret),
        "symlink escaped the mount:\n{out}"
    );
}

#[test]
#[ignore = "needs Apple container; run with make test-integration"]
fn only_project_and_home_are_host_mounts() {
    let sb = sandbox();
    let (ok, out) = run_in_box(&sb, "grep virtiofs /proc/mounts");
    assert!(ok, "{out}");
    let targets: Vec<String> = out
        .lines()
        .filter(|l| l.starts_with("virtiofs "))
        .map(|l| l.split(' ').nth(1).unwrap().replace("\\040", " "))
        .collect();
    let mut expected = vec![
        "/home/agent".to_string(),
        sb.project.to_string_lossy().to_string(),
    ];
    expected.sort();
    let mut got = targets.clone();
    got.sort();
    assert_eq!(got, expected, "I1: unexpected host mounts:\n{out}");
}

#[test]
#[ignore = "needs Apple container; run with make test-integration"]
fn home_is_the_dedicated_state_dir() {
    let sb = sandbox();
    let (ok, out) = run_in_box(&sb, "echo x > \"$HOME/marker\" && id -u");
    assert!(ok, "{out}");
    assert!(sb.state.join("marker").is_file());
    assert!(
        out.contains(&nix::unistd::getuid().as_raw().to_string()),
        "{out}"
    );
}

#[test]
#[ignore = "needs Apple container; run with make test-integration"]
fn cap_drop_all_still_allows_normal_work() {
    let sb = sandbox();
    let (ok, out) = run_in_box(
        &sb,
        "touch /tmp/x && mkdir -p \"$PWD/d\" && rm -r \"$PWD/d\" && echo ok",
    );
    assert!(ok && out.contains("ok"), "{out}");
}

// ---------- real lifecycle through `zevbox launch` ----------

/// "Agent" that consumes stdin and exits on EOF, like a real ACP adapter.
const READER: [&str; 3] = ["sh", "-c", "while read -r l; do :; done"];

fn real_env() -> TestEnv {
    ensure_base_image();
    TestEnv::new_real(container_bin(), BASE, &READER)
}

fn zevbox_names() -> Vec<String> {
    all_containers()
        .into_iter()
        .map(|(n, _)| n)
        .filter(|n| n.starts_with("zevbox-claude-"))
        .collect()
}

async fn wait_for(limit: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < limit {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    false
}

#[tokio::test]
#[ignore = "needs Apple container; run with make test-integration"]
async fn real_eof_removes_container() {
    let foreign = foreign_snapshot();
    let before = zevbox_names();
    let env = real_env();
    let mut l = env.launch("normal");
    assert!(
        wait_for(Duration::from_secs(30), || zevbox_names().len()
            > before.len())
        .await,
        "container never started"
    );
    let name = zevbox_names()
        .into_iter()
        .find(|n| !before.contains(n))
        .unwrap();
    let _c = Cleanup(name.clone());
    l.close_stdin();
    l.wait(Duration::from_secs(20)).await;
    assert!(
        wait_for(Duration::from_secs(15), || !zevbox_names().contains(&name)).await,
        "{name} still exists"
    );
    assert!(env.instance_records().is_empty());
    assert_eq!(
        foreign_snapshot(),
        foreign,
        "pre-existing containers changed"
    );
}

#[tokio::test]
#[ignore = "needs Apple container; run with make test-integration"]
async fn real_sigkill_of_zevbox_still_removes_container() {
    let foreign = foreign_snapshot();
    let before = zevbox_names();
    let env = real_env();
    let mut l = env.launch("normal");
    assert!(
        wait_for(Duration::from_secs(30), || zevbox_names().len()
            > before.len())
        .await,
        "container never started"
    );
    let name = zevbox_names()
        .into_iter()
        .find(|n| !before.contains(n))
        .unwrap();
    let _c = Cleanup(name.clone());
    // Zed may kill the agent process abruptly (F12): no cleanup code runs.
    l.child.start_kill().unwrap();
    l.wait(Duration::from_secs(5)).await;
    assert!(
        wait_for(Duration::from_secs(15), || !zevbox_names().contains(&name)).await,
        "{name} survived SIGKILL of zevbox: the VM must exit when its stdin closes"
    );
    // The stale record is cleaned by the next `clean` (or launch).
    let out = env.run(&["clean"]).await;
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    assert!(env.instance_records().is_empty());
    assert_eq!(
        foreign_snapshot(),
        foreign,
        "pre-existing containers changed"
    );
}

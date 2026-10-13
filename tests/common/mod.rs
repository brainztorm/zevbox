//! Shared helpers for end-to-end tests of the real `zevbox` binary with the
//! fake `container` CLI and the fake agent. PROTECTED FILE: do not edit.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(dead_code)]

use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout};

pub const RELEASE: &str = "claude-test";
pub const IMAGE: &str = "zevbox/claude:claude-test";

pub fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_zevbox"))
}

pub struct TestEnv {
    pub _tmp: tempfile::TempDir,
    pub root: PathBuf,
    pub home: PathBuf,
    pub state_root: PathBuf,
    pub project: PathBuf,
    pub fake_dir: PathBuf,
    pub container_log: PathBuf,
    pub agent_log: PathBuf,
    pub zed_settings: PathBuf,
    pub installed_binary: PathBuf,
    /// true: the real `container` CLI is used and HOME is the real one
    /// (the CLI needs it to reach its services).
    pub real: bool,
}

impl TestEnv {
    /// A configured installation: config with one allowed project, an active
    /// Claude release whose image exists, and Claude "authenticated".
    pub fn new() -> Self {
        Self::configured(
            manifest_dir().join("tests/fixtures/fake-container"),
            IMAGE,
            &["/opt/zevbox/claude/node_modules/.bin/claude-agent-acp"],
        )
    }

    /// Same layout, but with the real `container` CLI, a real local image and
    /// a real command used as "agent" (integration tests only).
    pub fn new_real(container_binary: PathBuf, image: &str, agent_command: &[&str]) -> Self {
        let mut env = Self::configured(container_binary, image, agent_command);
        env.real = true;
        env
    }

    fn configured(container_binary: PathBuf, image: &str, agent_command: &[&str]) -> Self {
        let env = Self::bare();
        fs::create_dir_all(&env.state_root).unwrap();
        let config = json!({
            "schemaVersion": 1,
            "containerBinary": container_binary,
            "projects": [env.project],
            "resources": {"cpus": 2, "memory": "1G"},
            "relay": {"maxMessageBytes": 1048576},
            "hostUid": 501,
            "hostGid": 20
        });
        write_json(&env.state_root.join("config.json"), &config);
        for d in [
            "instances",
            "logs",
            "backups",
            "state/claude/.claude",
            "state/antigravity",
            "releases/claude-test",
        ] {
            fs::create_dir_all(env.state_root.join(d)).unwrap();
        }
        write_json(
            &env.state_root.join("releases/active.json"),
            &json!({"claude": {"current": RELEASE, "previous": null}}),
        );
        write_json(
            &env.state_root.join("releases/claude-test/lock.json"),
            &json!({
                "schemaVersion": 1,
                "releaseId": RELEASE,
                "agent": "claude",
                "createdAt": "2026-10-13T10:15:00Z",
                "registryManifestUrl": "https://example.invalid/claude-acp/agent.json",
                "registryManifestSha256": "00",
                "agentVersion": "0.0.0-test",
                "baseImage": "node:22-bookworm-slim@sha256:00",
                "aptPackages": [],
                "npmPackage": "@agentclientprotocol/claude-agent-acp@0.0.0-test",
                "archiveUrl": null,
                "archiveSha256": null,
                "agentCommand": agent_command,
                "authCommand": ["/opt/zevbox/claude/node_modules/.bin/claude-agent-acp", "--cli", "auth", "login", "--claudeai"],
                "authMarker": ".claude/.credentials.json",
                "image": image,
                "smokeTest": {"passed": true, "at": "2026-10-13T10:16:00Z", "details": "test"}
            }),
        );
        fs::write(
            env.state_root
                .join("state/claude/.claude/.credentials.json"),
            "{}",
        )
        .unwrap();
        env
    }

    /// Directories only (home, project); no zevbox state at all.
    pub fn bare() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(tmp.path()).unwrap();
        let home = root.join("home");
        let project = home.join("code/proj");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(root.join("zed")).unwrap();
        Self {
            state_root: root.join("state-root"),
            fake_dir: root.join("fake-containers"),
            container_log: root.join("container.log"),
            agent_log: root.join("agent.log"),
            zed_settings: root.join("zed/settings.json"),
            installed_binary: home.join(".local/bin/zevbox"),
            home,
            project,
            root,
            _tmp: tmp,
            real: false,
        }
    }

    fn apply_env(&self, cmd: &mut tokio::process::Command, mode: &str) {
        let home = if self.real {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| self.home.clone())
        } else {
            self.home.clone()
        };
        cmd.env_clear()
            // Zed starts agents with a minimal PATH: zevbox must not depend on it.
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", home)
            .env("ZEVBOX_STATE_ROOT", &self.state_root)
            .env("ZEVBOX_ZED_SETTINGS", &self.zed_settings)
            .env("ZEVBOX_INSTALLED_BINARY", &self.installed_binary)
            .env("FAKE_CONTAINER_DIR", &self.fake_dir)
            .env("FAKE_CONTAINER_LOG", &self.container_log)
            .env(
                "FAKE_AGENT",
                manifest_dir().join("tests/fixtures/fake-agent.py"),
            )
            .env("FAKE_AGENT_LOG", &self.agent_log)
            .env("FAKE_AGENT_MODE", mode);
    }

    /// Runs a non-interactive zevbox command to completion.
    pub async fn run(&self, args: &[&str]) -> Output {
        let mut cmd = tokio::process::Command::new(bin());
        self.apply_env(&mut cmd, "normal");
        cmd.args(args).stdin(Stdio::null());
        let out = tokio::time::timeout(Duration::from_secs(30), cmd.output())
            .await
            .expect("command timed out")
            .unwrap();
        Output {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    /// Starts `zevbox launch claude --project <project>` like Zed does.
    pub fn launch(&self, mode: &str) -> Launch {
        self.launch_with(mode, &self.project.to_string_lossy())
    }

    pub fn launch_with(&self, mode: &str, project: &str) -> Launch {
        self.launch_with_env(mode, project, &[])
    }

    pub fn launch_with_env(&self, mode: &str, project: &str, extra: &[(&str, &str)]) -> Launch {
        let mut cmd = tokio::process::Command::new(bin());
        self.apply_env(&mut cmd, mode);
        for (k, v) in extra {
            cmd.env(k, v);
        }
        cmd.args(["launch", "claude", "--project", project])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // inherited: visible in `cargo test -- --nocapture`, never blocks on a full pipe
            .stderr(Stdio::inherit())
            .kill_on_drop(true);
        let mut child = cmd.spawn().unwrap();
        let stdin = child.stdin.take();
        let stdout = BufReader::new(child.stdout.take().unwrap()).lines();
        Launch {
            child,
            stdin,
            stdout,
        }
    }

    /// Current fake containers as (name, state), via the fake CLI itself.
    pub fn containers(&self) -> Vec<(String, String)> {
        let out = std::process::Command::new(manifest_dir().join("tests/fixtures/fake-container"))
            .args(["list", "--all", "--format", "json"])
            .env("FAKE_CONTAINER_DIR", &self.fake_dir)
            .env("FAKE_CONTAINER_LOG", self.root.join("helper.log"))
            .output()
            .unwrap();
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        v.as_array()
            .unwrap()
            .iter()
            .map(|c| {
                (
                    c["configuration"]["id"].as_str().unwrap().to_string(),
                    c["status"]["state"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }

    pub fn zevbox_containers(&self) -> Vec<String> {
        self.containers()
            .into_iter()
            .map(|(n, _)| n)
            .filter(|n| n.starts_with("zevbox-"))
            .collect()
    }

    /// Lines of the fake container log (one invocation per line).
    pub fn container_calls(&self) -> Vec<String> {
        fs::read_to_string(&self.container_log)
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }

    /// Messages received by the fake agent.
    pub fn agent_received(&self) -> Vec<Value> {
        fs::read_to_string(&self.agent_log)
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .collect()
    }

    pub fn instance_records(&self) -> Vec<PathBuf> {
        match fs::read_dir(self.state_root.join("instances")) {
            Ok(rd) => rd
                .map(|e| e.unwrap().path())
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// A pre-existing, non-zevbox container that must never be touched (F9).
    pub fn add_foreign_container(&self, name: &str) -> ProcGuard {
        let sleeper = std::process::Command::new("/bin/sleep")
            .arg("120")
            .spawn()
            .unwrap();
        let d = self.fake_dir.join(name);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("pid"), sleeper.id().to_string()).unwrap();
        fs::write(d.join("labels.json"), "{}").unwrap();
        ProcGuard(sleeper)
    }

    /// A fake container with zevbox name + managed label but no instance record.
    pub fn add_unregistered_zevbox_container(&self, name: &str) -> ProcGuard {
        let sleeper = std::process::Command::new("/bin/sleep")
            .arg("120")
            .spawn()
            .unwrap();
        let d = self.fake_dir.join(name);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("pid"), sleeper.id().to_string()).unwrap();
        fs::write(
            d.join("labels.json"),
            r#"{"zevbox.managed":"1","zevbox.instance":"cafebabe","zevbox.agent":"claude"}"#,
        )
        .unwrap();
        ProcGuard(sleeper)
    }
}

/// Kills and reaps the helper process when dropped (even if the test panics).
pub struct ProcGuard(pub std::process::Child);

impl ProcGuard {
    pub fn kill(&mut self) -> std::io::Result<()> {
        self.0.kill()?;
        self.0.wait().map(|_| ())
    }
}

impl Drop for ProcGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub struct Output {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub struct Launch {
    pub child: Child,
    pub stdin: Option<ChildStdin>,
    pub stdout: Lines<BufReader<ChildStdout>>,
}

impl Launch {
    pub async fn send(&mut self, msg: Value) {
        let stdin = self.stdin.as_mut().expect("stdin already closed");
        stdin
            .write_all(format!("{msg}\n").as_bytes())
            .await
            .unwrap();
        stdin.flush().await.unwrap();
    }

    /// Next line from zevbox stdout, parsed as JSON (fails the test otherwise).
    pub async fn recv(&mut self) -> Value {
        let line = tokio::time::timeout(Duration::from_secs(10), self.stdout.next_line())
            .await
            .expect("no message from zevbox within 10 s")
            .unwrap()
            .expect("zevbox closed stdout");
        serde_json::from_str(&line)
            .unwrap_or_else(|_| panic!("I6: non-JSON line on stdout: {line:?}"))
    }

    /// None if stdout reached EOF.
    pub async fn try_recv(&mut self, wait: Duration) -> Option<Value> {
        match tokio::time::timeout(wait, self.stdout.next_line()).await {
            Ok(Ok(Some(line))) => Some(
                serde_json::from_str(&line)
                    .unwrap_or_else(|_| panic!("I6: non-JSON line on stdout: {line:?}")),
            ),
            _ => None,
        }
    }

    pub fn close_stdin(&mut self) {
        self.stdin.take();
    }

    pub fn pid(&self) -> i32 {
        self.child.id().expect("process already reaped") as i32
    }

    /// Waits for exit; returns (exit code, elapsed). Fails after `limit`.
    pub async fn wait(&mut self, limit: Duration) -> (Option<i32>, Duration) {
        let start = Instant::now();
        let status = tokio::time::timeout(limit, self.child.wait())
            .await
            .unwrap_or_else(|_| panic!("zevbox launch did not exit within {limit:?}"))
            .unwrap();
        (status.code(), start.elapsed())
    }

    /// Standard handshake; returns the initialize response seen by "Zed".
    pub async fn initialize(&mut self) -> Value {
        self.send(json!({
            "jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": {
                "protocolVersion": 1,
                "clientCapabilities": {"fs": {"readTextFile": true, "writeTextFile": true}, "terminal": true, "auth": {"terminal": true}, "_meta": {"terminal-auth": true}},
                "clientInfo": {"name": "zed", "version": "test"}
            }
        }))
        .await;
        self.recv().await
    }
}

pub fn write_json(path: &Path, v: &Value) {
    fs::write(path, serde_json::to_string_pretty(v).unwrap()).unwrap();
}

/// Polls `cond` every 100 ms until true or `limit`.
pub async fn eventually(limit: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < limit {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    cond()
}

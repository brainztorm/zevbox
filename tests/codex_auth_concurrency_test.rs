//! Real processes, synthetic credentials, deterministic rendezvous. Not protected.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use common::{bin, write_json, TestEnv};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Attempt {
    child: Child,
    control: PathBuf,
}
impl Drop for Attempt {
    fn drop(&mut self) {
        // Also reap the fake container surviving a killed parent.
        let _ = nix::sys::signal::killpg(
            nix::unistd::Pid::from_raw(self.child.id() as i32),
            nix::sys::signal::Signal::SIGKILL,
        );
        let _ = self.child.wait();
    }
}
fn wait_for(mut f: impl FnMut() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "rendezvous timed out"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
impl Attempt {
    fn home(&self) -> PathBuf {
        wait_for(|| self.control.join("ready").exists());
        PathBuf::from(fs::read_to_string(self.control.join("ready")).unwrap())
    }
    fn finish(&mut self, code: i32) -> i32 {
        fs::write(self.control.join("exit"), code.to_string()).unwrap();
        wait_for(|| self.child.try_wait().unwrap().is_some());
        let status = self.child.wait().unwrap();
        for stream in ["stdout", "stderr"] {
            let text = fs::read_to_string(self.control.join(stream)).unwrap();
            assert!(!text.contains("candidate-"), "{text}");
        }
        status.code().unwrap()
    }
}
struct Harness {
    env: TestEnv,
    lock: PathBuf,
    bytes: Vec<u8>,
}
impl Harness {
    fn new() -> Self {
        let env = TestEnv::new();
        let fake = env.root.join("container");
        fs::write(&fake, r#"#!/usr/bin/python3
import sys, os, json, time
from pathlib import Path
a = sys.argv[1:]
if a[:2] == ['system', 'status']:
    print('{"status":"running"}')
elif a[:2] == ['image', 'list']:
    if os.environ.get('CACHE_MODE') == 'spawn-fail': os.unlink(sys.argv[0])
    print('[{"configuration":{"name":"zevbox/claude:claude-test"},"id":"test"}]')
elif a[0] == 'run':
    c = Path(os.environ['CONTROL'])
    state = next(x.split('source=',1)[1].split(',target=')[0] for x in a if 'target=/home/agent' in x)
    home = next(x.split('=',1)[1] for x in a if x.startswith('CODEX_HOME='))
    p = Path(state) / home.removeprefix('/home/agent/')
    assert p.is_dir()
    (c/'args').write_text(json.dumps(a))
    (c/'pid').write_text(str(os.getpid()))
    mode = os.environ.get('CACHE_MODE', 'valid')
    if mode == 'link':
        (p/'auth.json').symlink_to(c/'source')
        (c/'source').write_text('{"auth_mode":"apikey","OPENAI_API_KEY":"candidate-link"}')
    else:
        body = {'empty':'', 'json':'{', 'incomplete':'{"auth_mode":"chatgpt"}'}.get(mode, json.dumps({'auth_mode':'apikey','OPENAI_API_KEY':'candidate-'+c.name}))
        (p/'auth.json').write_text(body)
    (c/'ready').write_text(str(p))
    deadline = time.monotonic()+20
    while not (c/'exit').exists():
        if time.monotonic()>deadline: sys.exit(99)
        time.sleep(.01)
    sys.exit(int((c/'exit').read_text()))
else:
    sys.exit(98)
"#).unwrap();
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
        let config = env.state_root.join("config.json");
        let mut v: Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
        v["containerBinary"] = json!(fake);
        write_json(&config, &v);
        let lock = env.state_root.join("releases/codex-test/lock.json");
        fs::create_dir_all(lock.parent().unwrap()).unwrap();
        let mut v: Value = serde_json::from_slice(
            &fs::read(env.state_root.join("releases/claude-test/lock.json")).unwrap(),
        )
        .unwrap();
        v["agent"] = json!("codex");
        v["releaseId"] = json!("codex-test");
        v["authCommand"] = json!(zevbox::cli::auth::codex_login_command("/preserved-adapter"));
        v["authMarker"] = json!(".codex/auth.json");
        write_json(&lock, &v);
        write_json(
            &env.state_root.join("releases/active.json"),
            &json!({"codex":{"current":"codex-test","previous":null}}),
        );
        fs::create_dir_all(env.state_root.join("state/codex/.codex")).unwrap();
        fs::write(
            env.state_root.join("state/codex/.codex/auth.json"),
            r#"{"auth_mode":"apikey","OPENAI_API_KEY":"old"}"#,
        )
        .unwrap();
        let bytes = fs::read(&lock).unwrap();
        Self { env, lock, bytes }
    }
    fn start(&self, id: &str, mode: &str) -> Attempt {
        let control = self.env.root.join(id);
        fs::create_dir(&control).unwrap();
        let mut cmd = Command::new(bin());
        cmd.args(["auth", "codex"])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &self.env.home)
            .env("ZEVBOX_STATE_ROOT", &self.env.state_root)
            .env("ZEVBOX_ZED_SETTINGS", &self.env.zed_settings)
            .env("ZEVBOX_INSTALLED_BINARY", &self.env.installed_binary)
            .env("CONTROL", &control)
            .env("CACHE_MODE", mode)
            .stdin(Stdio::null())
            .stdout(fs::File::create(control.join("stdout")).unwrap())
            .stderr(fs::File::create(control.join("stderr")).unwrap())
            .process_group(0);
        Attempt {
            child: cmd.spawn().unwrap(),
            control,
        }
    }
    fn active(&self) -> String {
        let p = self.env.state_root.join("state/codex/.codex/auth.json");
        let v: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
        v["OPENAI_API_KEY"].as_str().unwrap().to_string()
    }
}
#[test]
fn two_attempts_and_both_publication_orders() {
    for reverse in [false, true] {
        let h = Harness::new();
        let fixed = h.env.state_root.join("state/codex/.codex-login");
        fs::create_dir(&fixed).unwrap();
        fs::write(fixed.join("sentinel"), "keep").unwrap();
        let mut a = h.start("A", "valid");
        let ah = a.home();
        let original = fs::read(ah.join("auth.json")).unwrap();
        let mut b = h.start("B", "valid");
        let bh = b.home();
        assert_eq!(
            fs::read(ah.join("auth.json")).unwrap(),
            original,
            "B deleted A's candidate"
        );
        assert_ne!(ah, bh, "both processes reuse the same HOME");
        for p in [&ah, &bh] {
            assert_eq!(fs::metadata(p).unwrap().permissions().mode() & 0o777, 0o700);
        }
        for attempt in [&a, &b] {
            let args: Vec<String> =
                serde_json::from_slice(&fs::read(attempt.control.join("args")).unwrap()).unwrap();
            assert_eq!(args.iter().filter(|s| *s == "--mount").count(), 2);
            assert!(args.contains(&"/preserved-adapter".to_string()));
        }
        let (first, second, one, two) = if reverse {
            (&mut b, &mut a, "B", "A")
        } else {
            (&mut a, &mut b, "A", "B")
        };
        assert_eq!(first.finish(0), 0);
        assert_eq!(h.active(), format!("candidate-{one}"));
        assert_eq!(second.finish(0), 0);
        assert_eq!(h.active(), format!("candidate-{two}"));
        assert!(!ah.exists());
        assert!(!bh.exists());
        assert_eq!(fs::read(&h.lock).unwrap(), h.bytes);
        assert_eq!(fs::read_to_string(fixed.join("sentinel")).unwrap(), "keep");
        assert_eq!(
            fs::metadata(h.env.state_root.join("state/codex/.codex/auth.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
#[test]
fn failures_do_not_publish_or_damage_other_attempts() {
    for first_success in [false, true] {
        let h = Harness::new();
        let mut a = h.start("A", "valid");
        a.home();
        let mut b = h.start("B", "valid");
        let bh = b.home();
        assert_eq!(
            a.finish(if first_success { 0 } else { 1 }),
            if first_success { 0 } else { 1 }
        );
        assert_eq!(
            h.active(),
            if first_success { "candidate-A" } else { "old" }
        );
        assert!(bh.join("auth.json").exists());
        assert_eq!(
            b.finish(if first_success { 1 } else { 0 }),
            if first_success { 1 } else { 0 }
        );
        assert_eq!(
            h.active(),
            if first_success {
                "candidate-A"
            } else {
                "candidate-B"
            }
        );
    }
    for mode in ["empty", "json", "incomplete", "link"] {
        let h = Harness::new();
        let mut a = h.start("A", mode);
        let p = a.home();
        assert_eq!(a.finish(0), 1);
        assert_eq!(h.active(), "old");
        assert!(!p.exists());
    }
}
#[test]
fn signals_isolate_attempts_and_leave_residues_unadopted() {
    use nix::{
        sys::signal::{kill, killpg, Signal},
        unistd::Pid,
    };
    for signal in [Signal::SIGINT, Signal::SIGKILL] {
        let h = Harness::new();
        let mut a = h.start("A", "valid");
        let ah = a.home();
        let mut b = h.start("B", "valid");
        b.home();
        if signal == Signal::SIGINT {
            killpg(Pid::from_raw(a.child.id() as i32), signal).unwrap();
        } else {
            kill(Pid::from_raw(a.child.id() as i32), signal).unwrap();
        }
        wait_for(|| a.child.try_wait().unwrap().is_some());
        assert_eq!(h.active(), "old");
        assert_eq!(b.finish(0), 0);
        assert_eq!(h.active(), "candidate-B");
        let residue = fs::read(ah.join("auth.json")).ok();
        let mut c = h.start("C", "valid");
        let ch = c.home();
        assert_ne!(ch, ah);
        assert_eq!(fs::read(ah.join("auth.json")).ok(), residue);
        assert_eq!(c.finish(0), 0);
        assert_eq!(h.active(), "candidate-C");
        if signal == Signal::SIGKILL {
            assert!(residue.is_some());
        }
    }
}
#[test]
fn incompatible_commands_are_rejected_before_run() {
    for command in [
        json!(["env", "/adapter"]),
        json!(["other", "CODEX_HOME=/home/agent/.codex-login", "/adapter"]),
        json!([
            "env",
            "X=1",
            "CODEX_HOME=/home/agent/.codex-login",
            "/adapter"
        ]),
        json!([
            "env",
            "CODEX_HOME=/home/agent/.codex-login",
            "/adapter",
            "CODEX_HOME=other"
        ]),
    ] {
        let h = Harness::new();
        let mut v: Value = serde_json::from_slice(&h.bytes).unwrap();
        v["authCommand"] = command;
        write_json(&h.lock, &v);
        let mut a = h.start("A", "valid");
        wait_for(|| a.child.try_wait().unwrap().is_some());
        assert_eq!(a.child.wait().unwrap().code(), Some(1));
        assert!(!a.control.join("ready").exists());
        assert!(fs::read_to_string(a.control.join("stderr"))
            .unwrap()
            .contains("unsupported Codex sign-in command"));
        assert_eq!(h.active(), "old");
    }
}

#[test]
fn spawn_failure_cleans_only_its_attempt() {
    let h = Harness::new();
    let mut b = h.start("B", "valid");
    let bh = b.home();
    let mut a = h.start("A", "spawn-fail");
    wait_for(|| a.child.try_wait().unwrap().is_some());
    assert_eq!(a.child.wait().unwrap().code(), Some(1));
    assert_eq!(h.active(), "old");
    assert!(bh.join("auth.json").exists());
    let dirs: Vec<_> = fs::read_dir(h.env.state_root.join("state/codex"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".codex-login-")
        })
        .collect();
    assert_eq!(dirs, vec![bh]);
    assert_eq!(b.finish(0), 0);
    assert_eq!(h.active(), "candidate-B");
}

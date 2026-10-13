//! §9.5 — config.json, project rules (I2), apt names, tools.json, fsutil.
//! PROTECTED FILE: do not edit.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use zevbox::config::{
    default_config, parse_config, validate_apt_package, validate_project_path, ConfigError,
    ProjectRules,
};
use zevbox::fsutil::{ensure_private_dir, prune_oldest, sha256_hex, write_atomic};
use zevbox::tools::ToolsFile;

const VALID: &str = r#"{
  "schemaVersion": 1,
  "containerBinary": "/opt/homebrew/bin/container",
  "projects": ["/Users/test/code/myproject"],
  "resources": { "cpus": 4, "memory": "4G" },
  "relay": { "maxMessageBytes": 33554432 },
  "hostUid": 501,
  "hostGid": 20
}"#;

fn with(field_path: &[&str], value: serde_json::Value) -> String {
    let mut v: serde_json::Value = serde_json::from_str(VALID).unwrap();
    let mut cur = &mut v;
    for (i, k) in field_path.iter().enumerate() {
        if i == field_path.len() - 1 {
            cur[*k] = value.clone();
        } else {
            cur = &mut cur[*k];
        }
    }
    v.to_string()
}

#[test]
fn parses_valid_config() {
    let c = parse_config(VALID).unwrap();
    assert_eq!(c.schema_version, 1);
    assert_eq!(
        c.projects,
        vec![PathBuf::from("/Users/test/code/myproject")]
    );
    assert_eq!(c.resources.memory, "4G");
    assert_eq!(c.relay.max_message_bytes, 33554432);
    assert_eq!((c.host_uid, c.host_gid), (501, 20));
}

#[test]
fn default_config_round_trips() {
    let c = default_config(PathBuf::from("/opt/homebrew/bin/container"), 501, 20);
    let text = serde_json::to_string_pretty(&c).unwrap();
    assert_eq!(parse_config(&text).unwrap(), c);
    assert!(c.projects.is_empty());
}

#[test]
fn rejects_unknown_fields() {
    let mut v: serde_json::Value = serde_json::from_str(VALID).unwrap();
    v["security"] = serde_json::json!({"failClosed": false});
    assert!(parse_config(&v.to_string()).is_err());
}

#[test]
fn rejects_wrong_schema_version() {
    assert!(parse_config(&with(&["schemaVersion"], 2.into())).is_err());
}

#[test]
fn rejects_relative_paths() {
    assert!(parse_config(&with(&["containerBinary"], "container".into())).is_err());
    assert!(parse_config(&with(&["projects"], serde_json::json!(["code/myproject"]))).is_err());
}

#[test]
fn rejects_comma_in_project() {
    assert!(parse_config(&with(&["projects"], serde_json::json!(["/a,b"]))).is_err());
}

#[test]
fn rejects_duplicate_projects() {
    assert!(parse_config(&with(&["projects"], serde_json::json!(["/a", "/a"]))).is_err());
}

#[test]
fn rejects_bad_resources() {
    assert!(parse_config(&with(&["resources", "cpus"], 0.into())).is_err());
    assert!(parse_config(&with(&["resources", "memory"], "".into())).is_err());
    assert!(parse_config(&with(&["relay", "maxMessageBytes"], 10.into())).is_err());
}

#[test]
fn rejects_invalid_json() {
    assert!(matches!(parse_config("{"), Err(ConfigError::Parse(_))));
}

// ---------- project rules (I2) ----------

struct Env {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    rules: ProjectRules,
}

/// Fake layout: <tmp>/home (HOME), <tmp>/home/code/proj (valid project),
/// <tmp>/state (forbidden), <tmp>/zevbox-src (forbidden).
fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    for d in ["home/code/proj/sub", "state/x", "zevbox-src", "home/a,b"] {
        fs::create_dir_all(root.join(d)).unwrap();
    }
    let rules = ProjectRules {
        home: root.join("home"),
        forbidden: vec![root.join("state"), root.join("zevbox-src")],
    };
    Env {
        _tmp: tmp,
        root,
        rules,
    }
}

#[test]
fn accepts_normal_project() {
    let e = env();
    let p = validate_project_path(&e.root.join("home/code/proj"), &e.rules).unwrap();
    assert_eq!(p, e.root.join("home/code/proj"));
}

#[test]
fn canonicalizes_symlinked_project() {
    let e = env();
    let link = e.root.join("home/link");
    std::os::unix::fs::symlink(e.root.join("home/code/proj"), &link).unwrap();
    assert_eq!(
        validate_project_path(&link, &e.rules).unwrap(),
        e.root.join("home/code/proj")
    );
    let dotted = e.root.join("home/code/proj/sub/..");
    assert_eq!(
        validate_project_path(&dotted, &e.rules).unwrap(),
        e.root.join("home/code/proj")
    );
}

#[test]
fn rejects_home_and_root_as_project() {
    let e = env();
    assert!(validate_project_path(Path::new("/"), &e.rules).is_err());
    assert!(validate_project_path(&e.root.join("home"), &e.rules).is_err());
    assert!(
        validate_project_path(&e.root, &e.rules).is_err(),
        "ancestor of HOME"
    );
}

#[test]
fn rejects_project_inside_state_root() {
    let e = env();
    assert!(validate_project_path(&e.root.join("state/x"), &e.rules).is_err());
    assert!(validate_project_path(&e.root.join("state"), &e.rules).is_err());
    assert!(validate_project_path(&e.root.join("zevbox-src"), &e.rules).is_err());
}

#[test]
fn rejects_project_containing_forbidden_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    fs::create_dir_all(root.join("home")).unwrap();
    fs::create_dir_all(root.join("work/inner-state")).unwrap();
    let rules = ProjectRules {
        home: root.join("home"),
        forbidden: vec![root.join("work/inner-state")],
    };
    assert!(validate_project_path(&root.join("work"), &rules).is_err());
}

#[test]
fn rejects_path_with_comma() {
    let e = env();
    assert!(validate_project_path(&e.root.join("home/a,b"), &e.rules).is_err());
}

#[test]
fn rejects_missing_or_file_project() {
    let e = env();
    assert!(validate_project_path(&e.root.join("home/nope"), &e.rules).is_err());
    let f = e.root.join("home/file.txt");
    fs::write(&f, "x").unwrap();
    assert!(validate_project_path(&f, &e.rules).is_err());
}

// ---------- apt names / tools.json ----------

#[test]
fn apt_name_validation() {
    for ok in [
        "ripgrep",
        "build-essential",
        "g++",
        "python3.11",
        "libssl3",
        "a",
    ] {
        assert!(validate_apt_package(ok).is_ok(), "{ok}");
    }
    for bad in [
        "",
        "Ripgrep",
        "-rf",
        "rg; rm -rf /",
        "a b",
        "$(id)",
        "pkg\n",
        "a/b",
        &"a".repeat(64),
    ] {
        assert!(validate_apt_package(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn tools_add_validates_and_dedups() {
    let mut t = ToolsFile::default();
    let added = t
        .add(&["ripgrep".into(), "make".into(), "ripgrep".into()])
        .unwrap();
    assert_eq!(added, vec!["ripgrep".to_string(), "make".to_string()]);
    assert_eq!(t.apt, vec!["ripgrep", "make"]);
    assert!(t.add(&["jq".into(), "bad name".into()]).is_err());
    assert_eq!(t.apt, vec!["ripgrep", "make"], "nothing added on error");
    assert_eq!(
        t.remove(&["make".into(), "unknown".into()]),
        vec!["make".to_string()]
    );
    assert_eq!(t.apt, vec!["ripgrep"]);
}

// ---------- fsutil ----------

#[test]
fn sha256_hex_known_value() {
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn write_atomic_creates_0600_and_preserves_mode() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("f.json");
    write_atomic(&p, b"one").unwrap();
    assert_eq!(fs::read(&p).unwrap(), b"one");
    assert_eq!(
        fs::metadata(&p).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap();
    write_atomic(&p, b"two").unwrap();
    assert_eq!(fs::read(&p).unwrap(), b"two");
    assert_eq!(
        fs::metadata(&p).unwrap().permissions().mode() & 0o777,
        0o644
    );
    assert_eq!(
        fs::read_dir(d.path()).unwrap().count(),
        1,
        "no temp file left behind"
    );
}

#[test]
fn ensure_private_dir_sets_0700() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("a/b");
    ensure_private_dir(&p).unwrap();
    assert_eq!(
        fs::metadata(&p).unwrap().permissions().mode() & 0o777,
        0o700
    );
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    ensure_private_dir(&p).unwrap();
    assert_eq!(
        fs::metadata(&p).unwrap().permissions().mode() & 0o777,
        0o700
    );
}

#[test]
fn prune_oldest_keeps_newest() {
    let d = tempfile::tempdir().unwrap();
    for i in 0..5 {
        let p = d.path().join(format!("{i}.log"));
        fs::write(&p, "x").unwrap();
        let t = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000 + i);
        fs::File::options()
            .write(true)
            .open(&p)
            .unwrap()
            .set_modified(t)
            .unwrap();
    }
    fs::write(d.path().join("keep.txt"), "x").unwrap();
    assert_eq!(prune_oldest(d.path(), ".log", 2).unwrap(), 3);
    let mut left: Vec<String> = fs::read_dir(d.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    left.sort();
    assert_eq!(left, vec!["3.log", "4.log", "keep.txt"]);
}

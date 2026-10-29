//! §9.3 — Zed JSONC settings editing. PROTECTED FILE: do not edit.
//! 2026-10-14: entries are one per agent (`zevbox · <Display>`, args
//! `["launch", <agent>]`), edited with the human's authorization (DECISIONS R1).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use jsonc_parser::ParseOptions;
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use zevbox::agents::AGENTS;
use zevbox::fsutil::sha256_hex;
use zevbox::zed_settings::{
    apply_entries, entries_for, install, write_if_unchanged, InstallOutcome, ManagedEntry,
    ZedSettingsError, MANAGED_PREFIX,
};

const BIN: &str = "/Users/test/.local/bin/zevbox";

fn fixture(name: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/zed")
        .join(name);
    fs::read_to_string(p).unwrap()
}

fn parse(text: &str) -> Value {
    jsonc_parser::parse_to_serde_value(text, &ParseOptions::default()).unwrap()
}

fn entry(name: &str, agent: &str) -> ManagedEntry {
    ManagedEntry {
        name: name.to_string(),
        command: BIN.to_string(),
        args: vec!["launch".into(), agent.into()],
    }
}

fn my_entries() -> Vec<ManagedEntry> {
    vec![
        entry("zevbox · Claude", "claude"),
        entry("zevbox · Antigravity", "antigravity"),
    ]
}

/// Comments of the fixture, each must survive verbatim.
const COMMENTS: [&str; 7] = [
    "// Zed settings",
    "// documentation: https://zed.dev/docs/configuring-zed",
    "/* Theme block\n     kept exactly as is */",
    "// trailing comment",
    "// Registry agent managed by Zed itself: zevbox must never touch it.",
    "/* inline block */",
    "// For information on how to configure Zed, see the Zed",
];

const CLAUDE_ACP_BLOCK: &str = r#""claude-acp": {
      "default_config_options": {
        "fast": true,
      },
    },"#;

// ---------- entries_for ----------

#[test]
fn entries_for_builds_names_and_args() {
    let got = entries_for(&AGENTS, BIN);
    let mut expected = my_entries();
    expected.push(entry("zevbox · oMLX", "omlx"));
    assert_eq!(got, expected);
}

#[test]
fn one_entry_per_agent_without_project_suffix() {
    let got = entries_for(&AGENTS[..1], BIN);
    let names: Vec<&str> = got.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["zevbox · Claude"]);
    assert!(
        got.iter().all(|e| !e.args.iter().any(|a| a == "--project")),
        "the project comes from the working directory Zed starts the agent in"
    );
}

#[test]
fn every_entry_name_has_managed_prefix() {
    let got = entries_for(&AGENTS, BIN);
    assert_eq!(got.len(), 3);
    assert!(got.iter().all(|e| e.name.starts_with(MANAGED_PREFIX)));
}

// ---------- apply_entries (pure) ----------

#[test]
fn install_semantics_match_expected() {
    let out = apply_entries(&fixture("with_comments.json"), &my_entries()).unwrap();
    let v = parse(&out);
    assert_eq!(v["theme"], "One Dark");
    assert_eq!(v["ui_font_size"], 16);
    assert_eq!(v["vim_mode"], false);
    let servers = v["agent_servers"].as_object().unwrap();
    let keys: Vec<&String> = servers.keys().collect();
    assert_eq!(
        keys,
        vec!["claude-acp", "zevbox · Claude", "zevbox · Antigravity"],
        "stale managed entry removed, unmanaged kept first, new ones appended in order"
    );
    assert_eq!(
        servers["claude-acp"],
        json!({"default_config_options": {"fast": true}})
    );
    assert_eq!(
        servers["zevbox · Claude"],
        json!({
            "type": "custom",
            "command": BIN,
            "args": ["launch", "claude"],
            "env": {}
        })
    );
}

#[test]
fn install_preserves_every_comment() {
    let out = apply_entries(&fixture("with_comments.json"), &my_entries()).unwrap();
    for c in COMMENTS {
        assert!(out.contains(c), "comment lost: {c}\n--- output ---\n{out}");
    }
}

#[test]
fn install_keeps_unmanaged_entries_byte_identical() {
    let input = fixture("with_comments.json");
    let out = apply_entries(&input, &my_entries()).unwrap();
    assert!(
        out.contains(CLAUDE_ACP_BLOCK),
        "claude-acp block must be byte-identical\n{out}"
    );
    // Everything before "agent_servers" is untouched.
    let head = &input[..input.find("\"agent_servers\"").unwrap()];
    assert!(
        out.starts_with(head),
        "text before agent_servers changed\n{out}"
    );
    assert!(out.contains("\"vim_mode\": false, /* inline block */"));
}

#[test]
fn install_is_idempotent_bytes() {
    let once = apply_entries(&fixture("with_comments.json"), &my_entries()).unwrap();
    let twice = apply_entries(&once, &my_entries()).unwrap();
    assert_eq!(once, twice);
}

#[test]
fn updates_changed_entry_in_place() {
    let once = apply_entries(&fixture("with_comments.json"), &my_entries()).unwrap();
    let mut changed = my_entries();
    changed[0].command = "/new/zevbox".into();
    let out = apply_entries(&once, &changed).unwrap();
    let v = parse(&out);
    assert_eq!(
        v["agent_servers"]["zevbox · Claude"]["command"],
        "/new/zevbox"
    );
    assert_eq!(
        v["agent_servers"].as_object().unwrap().len(),
        3,
        "no duplicate"
    );
}

#[test]
fn removes_stale_managed_entries() {
    // Also the migration path: the pre-2026-10-14 per-project entries
    // (`zevbox · Claude · old`, `--project`) are managed, hence replaced.
    let out = apply_entries(&fixture("with_comments.json"), &my_entries()).unwrap();
    assert!(!out.contains("zevbox · Claude · old"));
    assert!(!out.contains("/old/project"));
    assert!(!out.contains("--project"));
}

#[test]
fn uninstall_restores_semantics() {
    let input = fixture("with_comments.json");
    let installed = apply_entries(&input, &my_entries()).unwrap();
    let removed = apply_entries(&installed, &[]).unwrap();
    let mut expected = parse(&input);
    expected["agent_servers"]
        .as_object_mut()
        .unwrap()
        .remove("zevbox · Claude · old");
    assert_eq!(parse(&removed), expected);
    for c in COMMENTS {
        assert!(removed.contains(c), "comment lost after uninstall: {c}");
    }
    assert!(!removed.contains(MANAGED_PREFIX));
}

#[test]
fn creates_agent_servers_when_missing() {
    let input = fixture("no_agent_servers.json");
    let out = apply_entries(&input, &my_entries()).unwrap();
    let v = parse(&out);
    assert_eq!(v["theme"], "One Light");
    assert_eq!(v["agent_servers"].as_object().unwrap().len(), 2);
    assert!(out.contains("// minimal"));
    assert!(out.contains("// keep me"));
}

#[test]
fn empty_document_creates_new_settings() {
    let out = apply_entries("", &my_entries()).unwrap();
    let v = parse(&out);
    assert_eq!(v["agent_servers"].as_object().unwrap().len(), 2);
}

#[test]
fn invalid_jsonc_is_refused() {
    let r = apply_entries(&fixture("invalid.json"), &my_entries());
    assert!(matches!(r, Err(ZedSettingsError::InvalidJsonc(_))), "{r:?}");
}

#[test]
fn agent_servers_not_object_is_refused() {
    let r = apply_entries(&fixture("agent_servers_not_object.json"), &my_entries());
    assert!(
        matches!(r, Err(ZedSettingsError::AgentServersNotObject)),
        "{r:?}"
    );
}

#[test]
fn root_not_object_is_refused() {
    let r = apply_entries("[1, 2]", &my_entries());
    assert!(matches!(r, Err(ZedSettingsError::RootNotObject)), "{r:?}");
}

// ---------- file operations ----------

fn temp_settings(content: &str, mode: u32) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("settings.json");
    fs::write(&settings, content).unwrap();
    fs::set_permissions(&settings, fs::Permissions::from_mode(mode)).unwrap();
    let backups = dir.path().join("backups");
    fs::create_dir(&backups).unwrap();
    (dir, settings, backups)
}

fn count_files(dir: &Path) -> usize {
    fs::read_dir(dir).unwrap().count()
}

#[test]
fn install_writes_backup_and_file() {
    let input = fixture("with_comments.json");
    let (_d, settings, backups) = temp_settings(&input, 0o600);
    match install(&settings, &backups, &my_entries()).unwrap() {
        InstallOutcome::Updated { backup } => {
            assert_eq!(
                fs::read_to_string(&backup).unwrap(),
                input,
                "backup = original bytes"
            );
            let name = backup.file_name().unwrap().to_str().unwrap().to_string();
            assert!(
                name.starts_with("zed-settings-") && name.ends_with(".json"),
                "{name}"
            );
            assert!(name.contains(&sha256_hex(input.as_bytes())[..8]), "{name}");
        }
        other => panic!("expected Updated, got {other:?}"),
    }
    let written = fs::read_to_string(&settings).unwrap();
    assert_eq!(written, apply_entries(&input, &my_entries()).unwrap());
}

#[test]
fn second_install_is_unchanged_without_backup() {
    let (_d, settings, backups) = temp_settings(&fixture("with_comments.json"), 0o600);
    install(&settings, &backups, &my_entries()).unwrap();
    let after_first = fs::read(&settings).unwrap();
    assert_eq!(count_files(&backups), 1);
    assert_eq!(
        install(&settings, &backups, &my_entries()).unwrap(),
        InstallOutcome::Unchanged
    );
    assert_eq!(fs::read(&settings).unwrap(), after_first);
    assert_eq!(count_files(&backups), 1, "no backup when nothing changes");
}

#[test]
fn preserves_file_mode_0600() {
    let (_d, settings, backups) = temp_settings(&fixture("with_comments.json"), 0o600);
    install(&settings, &backups, &my_entries()).unwrap();
    let mode = fs::metadata(&settings).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn preserves_file_mode_0644() {
    let (_d, settings, backups) = temp_settings(&fixture("with_comments.json"), 0o644);
    install(&settings, &backups, &my_entries()).unwrap();
    let mode = fs::metadata(&settings).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o644);
}

#[test]
fn creates_file_when_missing() {
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("settings.json");
    let backups = dir.path().join("backups");
    fs::create_dir(&backups).unwrap();
    install(&settings, &backups, &my_entries()).unwrap();
    let v = parse(&fs::read_to_string(&settings).unwrap());
    assert_eq!(v["agent_servers"].as_object().unwrap().len(), 2);
}

#[test]
fn invalid_jsonc_file_is_left_untouched() {
    let input = fixture("invalid.json");
    let (_d, settings, backups) = temp_settings(&input, 0o600);
    assert!(install(&settings, &backups, &my_entries()).is_err());
    assert_eq!(fs::read_to_string(&settings).unwrap(), input);
}

#[test]
fn aborts_if_file_changed_concurrently() {
    let input = fixture("with_comments.json");
    let (_d, settings, _backups) = temp_settings(&input, 0o600);
    let expected = sha256_hex(input.as_bytes());
    fs::write(&settings, "{ \"changed\": true }").unwrap();
    let r = write_if_unchanged(&settings, &expected, "{}");
    assert!(
        matches!(r, Err(ZedSettingsError::ConcurrentModification)),
        "{r:?}"
    );
    assert_eq!(
        fs::read_to_string(&settings).unwrap(),
        "{ \"changed\": true }"
    );
}

#[test]
fn write_if_unchanged_writes_when_hash_matches() {
    let input = fixture("no_agent_servers.json");
    let (_d, settings, _b) = temp_settings(&input, 0o600);
    write_if_unchanged(&settings, &sha256_hex(input.as_bytes()), "{}\n").unwrap();
    assert_eq!(fs::read_to_string(&settings).unwrap(), "{}\n");
}

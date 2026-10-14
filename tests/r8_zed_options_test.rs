//! DECISIONS R8 — `default_config_options` written by Zed in a managed entry
//! survive `apply_entries`; a new entry is created without options (copying them from the registry entry
//! would contradict the protected `install_semantics_match_expected`).
//! Not in the protected manifest.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use jsonc_parser::ParseOptions;
use serde_json::{json, Value};
use zevbox::zed_settings::{apply_entries, ManagedEntry};

fn parse(text: &str) -> Value {
    jsonc_parser::parse_to_serde_value(text, &ParseOptions::default()).unwrap()
}

fn claude(command: &str) -> ManagedEntry {
    ManagedEntry {
        name: "zevbox · Claude".to_string(),
        command: command.to_string(),
        args: vec!["launch".into(), "claude".into()],
    }
}

const WITH_OPTIONS: &str = r#"{
  "agent_servers": {
    "zevbox · Claude": {
      "default_config_options": {"model": "haiku", "mode": "auto"},
      "type": "custom",
      "command": "/old",
      "args": ["launch", "claude"],
      "env": {}
    }
  }
}
"#;

#[test]
fn update_keeps_default_config_options() {
    let out = apply_entries(WITH_OPTIONS, &[claude("/new")]).unwrap();
    let entry = &parse(&out)["agent_servers"]["zevbox · Claude"];
    assert_eq!(
        entry["default_config_options"],
        json!({"model": "haiku", "mode": "auto"})
    );
    assert_eq!(entry["command"], "/new");
}

#[test]
fn conforming_entry_is_byte_identical() {
    let out = apply_entries(WITH_OPTIONS, &[claude("/old")]).unwrap();
    assert_eq!(out, WITH_OPTIONS);
}

#[test]
fn removal_drops_the_whole_entry() {
    let out = apply_entries(WITH_OPTIONS, &[]).unwrap();
    assert!(!out.contains("zevbox · Claude"));
    assert!(!out.contains("default_config_options"));
}

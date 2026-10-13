//! §8.7 — registry parsing, active releases, rollback. PROTECTED FILE: do not edit.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use chrono::TimeZone;
use std::fs;
use std::path::Path;
use zevbox::paths::Paths;
use zevbox::release::{
    load_active, new_release_id, parse_antigravity_manifest, parse_claude_manifest, promote,
    rollback, save_active, ActiveEntry, ActiveReleases, ReleaseError,
};

fn fixture(name: &str) -> String {
    fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/registry")
            .join(name),
    )
    .unwrap()
}

#[test]
fn parses_real_claude_manifest() {
    let c = parse_claude_manifest(&fixture("claude-acp.json")).unwrap();
    assert!(!c.version.is_empty());
    assert_eq!(
        c.npm_package,
        format!("@agentclientprotocol/claude-agent-acp@{}", c.version)
    );
}

#[test]
fn claude_manifest_without_npx_is_schema_error() {
    assert!(matches!(
        parse_claude_manifest(&fixture("claude-no-npx.json")),
        Err(ReleaseError::RegistrySchema(_))
    ));
}

#[test]
fn claude_package_must_match_version() {
    assert!(matches!(
        parse_claude_manifest(&fixture("claude-version-mismatch.json")),
        Err(ReleaseError::RegistrySchema(_))
    ));
}

#[test]
fn claude_parser_rejects_antigravity_manifest() {
    assert!(parse_claude_manifest(&fixture("antigravity-acp.json")).is_err());
}

#[test]
fn parses_real_antigravity_manifest() {
    let a = parse_antigravity_manifest(&fixture("antigravity-acp.json")).unwrap();
    assert!(a.archive_url.starts_with("https://"));
    assert!(
        a.archive_url.contains("linux"),
        "must pick linux-aarch64: {}",
        a.archive_url
    );
    assert_eq!(a.cmd, "./agy_acp_server.par");
    assert_eq!(a.args, vec!["--uid=".to_string()]);
}

#[test]
fn antigravity_without_linux_arm64_is_unavailable() {
    assert!(matches!(
        parse_antigravity_manifest(&fixture("antigravity-no-linux-arm64.json")),
        Err(ReleaseError::RegistrySchema(_))
    ));
}

#[test]
fn antigravity_wrong_id_rejected() {
    assert!(parse_antigravity_manifest(&fixture("antigravity-wrong-id.json")).is_err());
}

#[test]
fn release_id_format() {
    let now = chrono::Utc
        .with_ymd_and_hms(2026, 10, 13, 10, 15, 0)
        .unwrap();
    assert_eq!(new_release_id("claude", now), "claude-20261013-101500");
}

#[test]
fn promote_and_rollback() {
    let mut a = ActiveReleases::new();
    promote(&mut a, "claude", "claude-1");
    assert_eq!(
        a["claude"],
        ActiveEntry {
            current: "claude-1".into(),
            previous: None
        }
    );
    assert!(rollback(&mut a, "claude").is_err(), "no previous yet");
    promote(&mut a, "claude", "claude-2");
    assert_eq!(
        a["claude"],
        ActiveEntry {
            current: "claude-2".into(),
            previous: Some("claude-1".into())
        }
    );
    rollback(&mut a, "claude").unwrap();
    assert_eq!(
        a["claude"],
        ActiveEntry {
            current: "claude-1".into(),
            previous: Some("claude-2".into())
        }
    );
    assert!(rollback(&mut a, "antigravity").is_err());
}

#[test]
fn active_file_round_trip_and_missing_is_empty() {
    let d = tempfile::tempdir().unwrap();
    let paths = Paths::new(d.path());
    assert!(load_active(&paths).unwrap().is_empty());
    fs::create_dir_all(paths.releases_dir()).unwrap();
    let mut a = ActiveReleases::new();
    promote(&mut a, "claude", "claude-1");
    save_active(&paths, &a).unwrap();
    assert_eq!(load_active(&paths).unwrap(), a);
    let raw: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(paths.active_file()).unwrap()).unwrap();
    assert_eq!(
        raw,
        serde_json::json!({"claude": {"current": "claude-1", "previous": null}})
    );
}

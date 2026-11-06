//! Codex registry parser and build context. Not in the protected manifest.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{json, Value};
use zevbox::release::{
    parse_claude_manifest, parse_codex_manifest, prepare_context, ReleaseLock,
    CONTAINERFILE_TEMPLATE,
};

fn manifest() -> Value {
    serde_json::from_str(include_str!("fixtures/registry/codex-acp.json")).unwrap()
}

fn lock(agent: &str, version: &str, npm: &str) -> ReleaseLock {
    ReleaseLock {
        schema_version: 1,
        release_id: format!("{agent}-test"),
        agent: agent.to_string(),
        created_at: "2026-11-06T10:00:00Z".to_string(),
        registry_manifest_url: "https://example.invalid/agent.json".to_string(),
        registry_manifest_sha256: "00".to_string(),
        agent_version: version.to_string(),
        base_image: "ubuntu@sha256:00".to_string(),
        apt_packages: Vec::new(),
        npm_package: Some(npm.to_string()),
        archive_url: None,
        archive_sha256: None,
        agent_command: vec!["/x".to_string()],
        auth_command: Vec::new(),
        auth_marker: ".codex/auth.json".to_string(),
        image: format!("zevbox/{agent}:{agent}-test"),
        smoke_test: None,
    }
}

#[test]
fn stable_block_is_used_and_preview_ignored() {
    let c = parse_codex_manifest(&manifest().to_string()).unwrap();
    assert_eq!(c.version, "2.5.0");
    assert_eq!(c.npm_package, "@agentclientprotocol/codex-acp@2.5.0");
}

#[test]
fn wrong_or_incomplete_manifests_are_refused() {
    let mut wrong_id = manifest();
    wrong_id["id"] = json!("claude-acp");
    let mut preview = manifest();
    preview["version"] = json!("2.5.1-preview.23");
    preview["distribution"]["npx"]["package"] =
        json!("@agentclientprotocol/codex-acp@2.5.1-preview.23");
    let mut other_package = manifest();
    other_package["distribution"]["npx"]["package"] = json!("evil@2.5.0");
    let mut mismatch = manifest();
    mismatch["distribution"]["npx"]["package"] = json!("@agentclientprotocol/codex-acp@2.4.0");
    let mut no_version = manifest();
    no_version.as_object_mut().unwrap().remove("version");
    for bad in [wrong_id, preview, other_package, mismatch, no_version] {
        assert!(parse_codex_manifest(&bad.to_string()).is_err(), "{bad}");
    }
    assert!(parse_claude_manifest(&manifest().to_string()).is_err());
}

#[test]
fn context_names_the_exact_adapter() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = dir.path().join("context");
    let l = lock("codex", "2.5.0", "@agentclientprotocol/codex-acp@2.5.0");
    prepare_context(&ctx, &l).unwrap();
    let pkg: Value =
        serde_json::from_str(&std::fs::read_to_string(ctx.join("package.json")).unwrap()).unwrap();
    assert_eq!(
        pkg["dependencies"],
        json!({"@agentclientprotocol/codex-acp": "2.5.0"})
    );
}

#[test]
fn claude_context_is_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = dir.path().join("context");
    let l = lock(
        "claude",
        "0.99.0",
        "@agentclientprotocol/claude-agent-acp@0.99.0",
    );
    prepare_context(&ctx, &l).unwrap();
    let pkg: Value =
        serde_json::from_str(&std::fs::read_to_string(ctx.join("package.json")).unwrap()).unwrap();
    assert_eq!(
        pkg["dependencies"],
        json!({"@agentclientprotocol/claude-agent-acp": "0.99.0"})
    );
}

#[test]
fn image_keeps_the_bundled_sandbox() {
    let stage = CONTAINERFILE_TEMPLATE
        .split("FROM base AS codex")
        .nth(1)
        .unwrap()
        .split("FROM base AS")
        .next()
        .unwrap();
    assert!(stage.contains("ENV NO_BROWSER=1"));
    assert!(stage.contains("rm -f /usr/bin/bwrap"));
    assert!(stage.contains("npm install --no-audit --no-fund"));
}

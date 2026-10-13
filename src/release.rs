//! Releases: registry resolution, lock files, build, smoke test, promotion,
//! rollback (§8.7).

use crate::paths::Paths;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SmokeTest {
    pub passed: bool,
    pub at: String,
    pub details: String,
}

/// `releases/<id>/lock.json` (§8.7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseLock {
    pub schema_version: u32,
    pub release_id: String,
    pub agent: String,
    pub created_at: String,
    pub registry_manifest_url: String,
    pub registry_manifest_sha256: String,
    pub agent_version: String,
    pub base_image: String,
    pub apt_packages: Vec<String>,
    pub npm_package: Option<String>,
    pub archive_url: Option<String>,
    pub archive_sha256: Option<String>,
    pub agent_command: Vec<String>,
    pub auth_command: Vec<String>,
    /// Path relative to `state/<agent>` whose existence means "authenticated".
    pub auth_marker: String,
    pub image: String,
    pub smoke_test: Option<SmokeTest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActiveEntry {
    pub current: String,
    pub previous: Option<String>,
}

/// `releases/active.json`: agent id → entry.
pub type ActiveReleases = BTreeMap<String, ActiveEntry>;

/// What the registry says for Claude.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeComponent {
    pub version: String,
    /// Exact `name@version` from `distribution.npx.package`.
    pub npm_package: String,
}

/// What the registry says for Antigravity (linux-aarch64 only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AntigravityComponent {
    pub version: String,
    pub archive_url: String,
    pub cmd: String,
    pub args: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ReleaseError {
    #[error("registry schema changed: {0}")]
    RegistrySchema(String),
    #[error("no active {0} release")]
    NoActiveRelease(String),
    #[error("release store error: {0}")]
    Store(String),
    #[error("not implemented yet: {0}")]
    NotImplemented(&'static str),
}

/// Requires `id == "claude-acp"`, `version`, `distribution.npx.package`
/// (which must end with `@<version>`). Ignores `preview`.
pub fn parse_claude_manifest(text: &str) -> Result<ClaudeComponent, ReleaseError> {
    let _ = text;
    Err(ReleaseError::NotImplemented(
        "release::parse_claude_manifest",
    ))
}

/// Requires `id == "antigravity-acp"`, `version`,
/// `distribution.binary["linux-aarch64"]` with `archive` (https URL) and `cmd`;
/// `args` defaults to empty. Never uses another platform.
pub fn parse_antigravity_manifest(text: &str) -> Result<AntigravityComponent, ReleaseError> {
    let _ = text;
    Err(ReleaseError::NotImplemented(
        "release::parse_antigravity_manifest",
    ))
}

/// Missing file → empty map.
pub fn load_active(paths: &Paths) -> Result<ActiveReleases, ReleaseError> {
    let _ = paths;
    Err(ReleaseError::NotImplemented("release::load_active"))
}

pub fn save_active(paths: &Paths, active: &ActiveReleases) -> Result<(), ReleaseError> {
    let _ = (paths, active);
    Err(ReleaseError::NotImplemented("release::save_active"))
}

pub fn load_lock(paths: &Paths, release_id: &str) -> Result<ReleaseLock, ReleaseError> {
    let _ = (paths, release_id);
    Err(ReleaseError::NotImplemented("release::load_lock"))
}

/// Lock of the `current` release of `agent`, or `NoActiveRelease`.
pub fn active_lock(paths: &Paths, agent: &str) -> Result<ReleaseLock, ReleaseError> {
    let _ = (paths, agent);
    Err(ReleaseError::NotImplemented("release::active_lock"))
}

/// Pure: new `current`, old current becomes `previous`.
pub fn promote(active: &mut ActiveReleases, agent: &str, release_id: &str) {
    let _ = (active, agent, release_id);
}

/// Pure: swaps `current` and `previous`; error if there is no previous.
pub fn rollback(active: &mut ActiveReleases, agent: &str) -> Result<(), ReleaseError> {
    let _ = (active, agent);
    Err(ReleaseError::NotImplemented("release::rollback"))
}

/// `<agent>-<YYYYMMDD-HHMMSS>` in UTC.
pub fn new_release_id(agent: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    let _ = (agent, now);
    String::new()
}

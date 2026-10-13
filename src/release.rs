//! Releases: registry resolution, lock files, build, smoke test, promotion,
//! rollback (§8.7).

use crate::fsutil::{ensure_private_dir, write_atomic};
use crate::paths::Paths;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
    let root: Value = serde_json::from_str(text)
        .map_err(|e| ReleaseError::RegistrySchema(format!("invalid JSON: {e}")))?;

    let id = root
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| ReleaseError::RegistrySchema("missing id field".to_string()))?;
    if id != "claude-acp" {
        return Err(ReleaseError::RegistrySchema(format!(
            "expected id 'claude-acp', got '{id}'"
        )));
    }

    let version = root
        .get("version")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| ReleaseError::RegistrySchema("missing or empty version".to_string()))?;

    let npm_package = root
        .get("distribution")
        .and_then(|d| d.get("npx"))
        .and_then(|n| n.get("package"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ReleaseError::RegistrySchema("missing distribution.npx.package".to_string())
        })?;

    let expected_suffix = format!("@{version}");
    if !npm_package.ends_with(&expected_suffix) {
        return Err(ReleaseError::RegistrySchema(format!(
            "npx package '{npm_package}' does not end with '@{version}'"
        )));
    }

    Ok(ClaudeComponent {
        version: version.to_string(),
        npm_package: npm_package.to_string(),
    })
}

/// Requires `id == "antigravity-acp"`, `version`,
/// `distribution.binary["linux-aarch64"]` with `archive` (https URL) and `cmd`;
/// `args` defaults to empty. Never uses another platform.
pub fn parse_antigravity_manifest(text: &str) -> Result<AntigravityComponent, ReleaseError> {
    let root: Value = serde_json::from_str(text)
        .map_err(|e| ReleaseError::RegistrySchema(format!("invalid JSON: {e}")))?;

    let id = root
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| ReleaseError::RegistrySchema("missing id field".to_string()))?;
    if id != "antigravity-acp" {
        return Err(ReleaseError::RegistrySchema(format!(
            "expected id 'antigravity-acp', got '{id}'"
        )));
    }

    let version = root
        .get("version")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| ReleaseError::RegistrySchema("missing or empty version".to_string()))?;

    let linux_arm = root
        .get("distribution")
        .and_then(|d| d.get("binary"))
        .and_then(|b| b.get("linux-aarch64"))
        .ok_or_else(|| {
            ReleaseError::RegistrySchema("missing distribution.binary['linux-aarch64']".to_string())
        })?;

    let archive_url = linux_arm
        .get("archive")
        .and_then(Value::as_str)
        .ok_or_else(|| ReleaseError::RegistrySchema("missing archive URL".to_string()))?;

    if !archive_url.starts_with("https://") {
        return Err(ReleaseError::RegistrySchema(
            "archive URL must use HTTPS".to_string(),
        ));
    }

    let cmd = linux_arm
        .get("cmd")
        .and_then(Value::as_str)
        .ok_or_else(|| ReleaseError::RegistrySchema("missing cmd".to_string()))?;

    let args = match linux_arm.get("args") {
        Some(Value::Array(arr)) => {
            let mut out = Vec::new();
            for a in arr {
                if let Some(s) = a.as_str() {
                    out.push(s.to_string());
                } else {
                    return Err(ReleaseError::RegistrySchema(
                        "args elements must be strings".to_string(),
                    ));
                }
            }
            out
        }
        _ => Vec::new(),
    };

    Ok(AntigravityComponent {
        version: version.to_string(),
        archive_url: archive_url.to_string(),
        cmd: cmd.to_string(),
        args,
    })
}

/// Missing file → empty map.
pub fn load_active(paths: &Paths) -> Result<ActiveReleases, ReleaseError> {
    let file = paths.active_file();
    match std::fs::read_to_string(&file) {
        Ok(content) => serde_json::from_str(&content)
            .map_err(|e| ReleaseError::Store(format!("invalid active releases JSON: {e}"))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ActiveReleases::new()),
        Err(e) => Err(ReleaseError::Store(format!(
            "failed to read active releases from {}: {e}",
            file.display()
        ))),
    }
}

pub fn save_active(paths: &Paths, active: &ActiveReleases) -> Result<(), ReleaseError> {
    ensure_private_dir(&paths.releases_dir())
        .map_err(|e| ReleaseError::Store(format!("failed to create releases dir: {e}")))?;
    let bytes = serde_json::to_vec_pretty(active)
        .map_err(|e| ReleaseError::Store(format!("failed to serialize active releases: {e}")))?;
    write_atomic(&paths.active_file(), &bytes)
        .map_err(|e| ReleaseError::Store(format!("failed to write active releases: {e}")))
}

pub fn load_lock(paths: &Paths, release_id: &str) -> Result<ReleaseLock, ReleaseError> {
    let file = paths.release_lock_file(release_id);
    match std::fs::read_to_string(&file) {
        Ok(content) => serde_json::from_str(&content)
            .map_err(|e| ReleaseError::Store(format!("invalid lock file {}: {e}", file.display()))),
        Err(e) => Err(ReleaseError::Store(format!(
            "failed to read lock file {}: {e}",
            file.display()
        ))),
    }
}

/// Lock of the `current` release of `agent`, or `NoActiveRelease`.
pub fn active_lock(paths: &Paths, agent: &str) -> Result<ReleaseLock, ReleaseError> {
    let active = load_active(paths)?;
    let entry = active
        .get(agent)
        .ok_or_else(|| ReleaseError::NoActiveRelease(agent.to_string()))?;
    load_lock(paths, &entry.current)
}

/// Pure: new `current`, old current becomes `previous`.
pub fn promote(active: &mut ActiveReleases, agent: &str, release_id: &str) {
    let previous = active.get(agent).map(|e| e.current.clone());
    active.insert(
        agent.to_string(),
        ActiveEntry {
            current: release_id.to_string(),
            previous,
        },
    );
}

/// Pure: swaps `current` and `previous`; error if there is no previous.
pub fn rollback(active: &mut ActiveReleases, agent: &str) -> Result<(), ReleaseError> {
    let entry = active
        .get_mut(agent)
        .ok_or_else(|| ReleaseError::NoActiveRelease(agent.to_string()))?;
    let prev = entry.previous.take().ok_or_else(|| {
        ReleaseError::Store(format!("no previous release to rollback for {agent}"))
    })?;
    let old_current = std::mem::replace(&mut entry.current, prev);
    entry.previous = Some(old_current);
    Ok(())
}

/// `<agent>-<YYYYMMDD-HHMMSS>` in UTC.
pub fn new_release_id(agent: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    format!("{agent}-{}", now.format("%Y%m%d-%H%M%S"))
}

//! `config.json` (§5.3): parsing, validation, atomic save. Project path rules (I2).
//!
//! WORKED EXAMPLE (step 3): typed errors (`thiserror`) that tell the user which
//! field to fix, validation separated from parsing, security rules commented
//! with the invariant they enforce.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub container_binary: PathBuf,
    /// Legacy allow-list (before 2026-10-16, DECISIONS D1/F1): no command reads
    /// or adds to it any more; kept so existing config.json files still load.
    pub projects: Vec<PathBuf>,
    pub resources: Resources,
    pub relay: RelayConfig,
    pub host_uid: u32,
    pub host_gid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Resources {
    pub cpus: u32,
    /// Passed verbatim to `container run --memory` (e.g. "4G").
    pub memory: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RelayConfig {
    pub max_message_bytes: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read or write {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("config.json is not valid: {0}")]
    Parse(String),
    #[error("config.json field '{field}' is invalid: {reason}")]
    Invalid { field: &'static str, reason: String },
    #[error("project {path} is not allowed: {reason}")]
    ProjectRejected { path: PathBuf, reason: String },
    #[error("'{0}' is not a valid apt package name")]
    InvalidAptName(String),
}

/// Inputs of the project rules, injected so tests do not depend on the real HOME.
#[derive(Debug, Clone)]
pub struct ProjectRules {
    /// The user's home directory.
    pub home: PathBuf,
    /// Directories a project may neither be inside of nor contain:
    /// the state root and the zevbox source/installation directories.
    pub forbidden: Vec<PathBuf>,
}

/// Default config written by `zevbox init` (projects empty).
pub fn default_config(container_binary: PathBuf, host_uid: u32, host_gid: u32) -> Config {
    Config {
        schema_version: SCHEMA_VERSION,
        container_binary,
        projects: Vec::new(),
        resources: Resources {
            cpus: 4,
            memory: "4G".to_string(),
        },
        relay: RelayConfig {
            max_message_bytes: 32 * 1024 * 1024,
        },
        host_uid,
        host_gid,
    }
}

/// Parses then validates (see [`validate`]). Unknown fields are errors.
pub fn parse_config(text: &str) -> Result<Config, ConfigError> {
    let config: Config =
        serde_json::from_str(text).map_err(|e| ConfigError::Parse(e.to_string()))?;
    validate(&config)?;
    Ok(config)
}

fn invalid(field: &'static str, reason: &str) -> ConfigError {
    ConfigError::Invalid {
        field,
        reason: reason.to_string(),
    }
}

/// Rules: `schemaVersion == 1`; `containerBinary` absolute; every project
/// absolute, without ',' and unique; `cpus >= 1`; `memory` non-empty;
/// `maxMessageBytes >= 1024`.
pub fn validate(config: &Config) -> Result<(), ConfigError> {
    if config.schema_version != SCHEMA_VERSION {
        return Err(invalid("schemaVersion", "must be 1"));
    }
    // Zed starts zevbox with a minimal PATH: the binary must be absolute (§5.3).
    if !config.container_binary.is_absolute() {
        return Err(invalid("containerBinary", "must be an absolute path"));
    }
    let mut seen = std::collections::HashSet::new();
    for project in &config.projects {
        if !project.is_absolute() {
            return Err(invalid(
                "projects",
                "every project must be an absolute path",
            ));
        }
        // A ',' would break `--mount type=bind,source=...` (§8.2).
        if project.to_string_lossy().contains(',') {
            return Err(invalid("projects", "a project path must not contain ','"));
        }
        if !seen.insert(project) {
            return Err(invalid("projects", "duplicate project"));
        }
    }
    if config.resources.cpus < 1 {
        return Err(invalid("resources.cpus", "must be at least 1"));
    }
    if config.resources.memory.trim().is_empty() {
        return Err(invalid(
            "resources.memory",
            "must not be empty (e.g. \"4G\")",
        ));
    }
    if config.relay.max_message_bytes < 1024 {
        return Err(invalid("relay.maxMessageBytes", "must be at least 1024"));
    }
    Ok(())
}

pub fn load(path: &Path) -> Result<Config, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse_config(&text)
}

/// Pretty JSON + trailing newline, written with `fsutil::write_atomic`.
pub fn save(path: &Path, config: &Config) -> Result<(), ConfigError> {
    validate(config)?;
    let mut text =
        serde_json::to_string_pretty(config).map_err(|e| ConfigError::Parse(e.to_string()))?;
    text.push('\n');
    crate::fsutil::write_atomic(path, text.as_bytes()).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// I2. Canonicalizes `path` (must exist and be a directory), then rejects:
/// a path containing ','; `/`; `home` itself or any ancestor of `home`;
/// a path inside a `forbidden` dir or containing one; a path outside the
/// allowed zones ([`zone_rejection`]). Returns the canonical path.
pub fn validate_project_path(path: &Path, rules: &ProjectRules) -> Result<PathBuf, ConfigError> {
    let reject = |reason: &str| ConfigError::ProjectRejected {
        path: path.to_path_buf(),
        reason: reason.to_string(),
    };
    // Canonical form resolves symlinks and `..`, so the checks below see the real target.
    let canonical =
        std::fs::canonicalize(path).map_err(|e| reject(&format!("cannot resolve it ({e})")))?;
    if !canonical.is_dir() {
        return Err(reject("it is not a directory"));
    }
    if canonical.to_string_lossy().contains(',') {
        return Err(reject(
            "the path contains ',' (unsupported by container mounts)",
        ));
    }
    if canonical == Path::new("/") {
        return Err(reject("mounting / would expose the whole Mac"));
    }
    // `home.starts_with(canonical)` ⇔ canonical is HOME or one of its parents (I1).
    let home = canonicalize_or_keep(&rules.home);
    if home.starts_with(&canonical) {
        return Err(reject("it is your home folder or one of its parents"));
    }
    // I12: zevbox's own files must never be reachable from the VM.
    for forbidden in &rules.forbidden {
        let forbidden = canonicalize_or_keep(forbidden);
        if canonical.starts_with(&forbidden) || forbidden.starts_with(&canonical) {
            return Err(reject(&format!(
                "it overlaps a zevbox directory ({})",
                forbidden.display()
            )));
        }
    }
    if let Some(reason) = zone_rejection(&canonical, &home) {
        return Err(reject(reason));
    }
    Ok(canonical)
}

/// Mount point of external disks on macOS.
const VOLUMES: &str = "/Volumes";

/// I2 zones (DECISIONS E1, 2026-10-16). The agent writes in its project, and
/// some folders hold files the Mac runs by itself (LaunchAgents, app and tool
/// settings, binaries on the PATH, `Zed.app`...). A project must therefore be
/// strictly inside `home` but not in `~/Library` nor in a hidden `~/.*`
/// folder, or strictly inside a disk under `/Volumes`. Pure path logic on
/// canonical paths; `None` means allowed.
pub fn zone_rejection(canonical: &Path, home: &Path) -> Option<&'static str> {
    if let Ok(relative) = canonical.strip_prefix(home) {
        let first = relative.components().next()?.as_os_str().to_string_lossy();
        if first == "Library" {
            return Some("it is inside ~/Library, where macOS and apps keep settings the Mac runs");
        }
        if first.starts_with('.') {
            return Some("it is inside a hidden folder of your home (~/.…), where tools keep settings the Mac runs");
        }
        return None;
    }
    if let Ok(relative) = canonical.strip_prefix(VOLUMES) {
        return match relative.components().count() {
            0 | 1 => Some("it is a whole disk; use a folder on that disk"),
            _ => None,
        };
    }
    Some("it is outside your home folder and external disks (system folders, /Applications, /opt/homebrew…)")
}

/// The fix shown with every project rejection.
pub const PROJECT_ZONES_FIX: &str =
    "Fix: open a folder inside your home folder (not ~/Library or a hidden ~/.… folder) or on an external disk.";

/// Rules may name directories that do not exist yet (e.g. ~/.local/bin).
fn canonicalize_or_keep(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// `^[a-z0-9][a-z0-9+.-]{0,62}$` (§8.8). The only barrier against injection
/// into the Containerfile `RUN` line.
pub fn validate_apt_package(name: &str) -> Result<(), ConfigError> {
    let bytes = name.as_bytes();
    let first_ok = bytes
        .first()
        .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
    let rest_ok = bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'+' | b'.' | b'-'));
    if first_ok && rest_ok && bytes.len() <= 63 {
        Ok(())
    } else {
        Err(ConfigError::InvalidAptName(name.to_string()))
    }
}

#[cfg(test)]
mod zone_tests {
    use super::*;

    fn rejected(p: &str) -> bool {
        zone_rejection(Path::new(p), Path::new("/Users/me")).is_some()
    }

    #[test]
    fn ordinary_projects_in_home_are_allowed() {
        assert!(!rejected("/Users/me/code/app"));
        assert!(!rejected("/Users/me/Documents/site"));
        assert!(
            !rejected("/Users/me/code/app/.hidden-sub"),
            "only ~/.* is refused"
        );
    }

    #[test]
    fn library_and_hidden_home_folders_are_refused() {
        for p in [
            "/Users/me/Library",
            "/Users/me/Library/LaunchAgents",
            "/Users/me/Library/Application Support/com.apple.container",
            "/Users/me/.ssh",
            "/Users/me/.config/zed",
            "/Users/me/.local/bin",
            "/Users/me/.aws",
        ] {
            assert!(rejected(p), "{p} must be refused");
        }
    }

    #[test]
    fn external_disks_are_allowed_but_not_whole() {
        assert!(!rejected("/Volumes/Data/code/app"));
        assert!(rejected("/Volumes/Data"));
        assert!(rejected("/Volumes"));
    }

    #[test]
    fn everything_else_is_refused() {
        for p in [
            "/Applications/Zed.app",
            "/opt/homebrew",
            "/usr/local/bin",
            "/private/tmp/x",
            "/private/etc",
            "/Users/Shared/x",
            "/Users/other/code",
        ] {
            assert!(rejected(p), "{p} must be refused");
        }
    }
}

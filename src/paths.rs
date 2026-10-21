//! Every on-disk location used by zevbox (§5.2). Do not build paths elsewhere.
//!
//! Tests override locations with `ZEVBOX_STATE_ROOT`, `ZEVBOX_ZED_SETTINGS` and
//! `ZEVBOX_INSTALLED_BINARY`.

use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum PathsError {
    #[error("HOME is not set")]
    NoHome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub state_root: PathBuf,
}

pub fn home_dir() -> Result<PathBuf, PathsError> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
        .ok_or(PathsError::NoHome)
}

impl Paths {
    pub fn new(state_root: impl Into<PathBuf>) -> Self {
        Self {
            state_root: state_root.into(),
        }
    }

    /// `ZEVBOX_STATE_ROOT` if set, else `~/Library/Application Support/zevbox`.
    /// If step 1 of the plan switched to `~/.local/share/zevbox`, change it here only.
    pub fn from_env() -> Result<Self, PathsError> {
        if let Some(root) = std::env::var_os("ZEVBOX_STATE_ROOT").filter(|r| !r.is_empty()) {
            return Ok(Self::new(root));
        }
        Ok(Self::new(
            home_dir()?.join("Library/Application Support/zevbox"),
        ))
    }

    pub fn config_file(&self) -> PathBuf {
        self.state_root.join("config.json")
    }
    pub fn tools_file(&self) -> PathBuf {
        self.state_root.join("tools.json")
    }
    pub fn state_dir(&self, agent: &str) -> PathBuf {
        self.state_root.join("state").join(agent)
    }
    pub fn instances_dir(&self) -> PathBuf {
        self.state_root.join("instances")
    }
    pub fn instance_file(&self, id: &str) -> PathBuf {
        self.instances_dir().join(format!("{id}.json"))
    }
    pub fn logs_dir(&self) -> PathBuf {
        self.state_root.join("logs")
    }
    pub fn log_file(&self, id: &str) -> PathBuf {
        self.logs_dir().join(format!("{id}.log"))
    }
    pub fn backups_dir(&self) -> PathBuf {
        self.state_root.join("backups")
    }
    pub fn releases_dir(&self) -> PathBuf {
        self.state_root.join("releases")
    }
    pub fn release_dir(&self, release_id: &str) -> PathBuf {
        self.releases_dir().join(release_id)
    }
    pub fn release_lock_file(&self, release_id: &str) -> PathBuf {
        self.release_dir(release_id).join("lock.json")
    }
    pub fn active_file(&self) -> PathBuf {
        self.releases_dir().join("active.json")
    }
    /// Directories created by `zevbox init`, all with mode 0700.
    pub fn all_dirs(&self) -> Vec<PathBuf> {
        vec![
            self.state_root.clone(),
            self.state_root.join("state"),
            self.state_dir("claude"),
            self.state_dir("antigravity"),
            self.instances_dir(),
            self.logs_dir(),
            self.backups_dir(),
            self.releases_dir(),
        ]
    }
}

/// I2 rules for this installation: the user's home, and zevbox's own folders
/// (state root, folder of the installed binary) as forbidden overlaps (I12).
pub fn project_rules(paths: &Paths) -> Result<crate::config::ProjectRules, PathsError> {
    let installed = installed_binary()?;
    let mut forbidden = vec![paths.state_root.clone()];
    if let Some(dir) = installed.parent() {
        forbidden.push(dir.to_path_buf());
    }
    Ok(crate::config::ProjectRules {
        home: home_dir()?,
        forbidden,
    })
}

/// `ZEVBOX_ZED_SETTINGS` if set, else `~/.config/zed/settings.json`.
pub fn zed_settings_file() -> Result<PathBuf, PathsError> {
    if let Some(p) = std::env::var_os("ZEVBOX_ZED_SETTINGS").filter(|p| !p.is_empty()) {
        return Ok(PathBuf::from(p));
    }
    Ok(home_dir()?.join(".config/zed/settings.json"))
}

/// Absolute path written into Zed entries: `ZEVBOX_INSTALLED_BINARY` if set,
/// else `~/.local/bin/zevbox` (§5.2). Never `target/release`.
pub fn installed_binary() -> Result<PathBuf, PathsError> {
    if let Some(p) = std::env::var_os("ZEVBOX_INSTALLED_BINARY").filter(|p| !p.is_empty()) {
        return Ok(PathBuf::from(p));
    }
    Ok(home_dir()?.join(".local/bin/zevbox"))
}

/// True if `inner` equals `outer` or is inside it (pure path comparison).
pub fn is_within(inner: &Path, outer: &Path) -> bool {
    inner.starts_with(outer)
}

//! `tools.json` (§8.8): extra apt packages baked into the images.

use crate::config::ConfigError;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolsFile {
    pub schema_version: u32,
    pub apt: Vec<String>,
}

impl Default for ToolsFile {
    fn default() -> Self {
        Self {
            schema_version: 1,
            apt: Vec::new(),
        }
    }
}

impl ToolsFile {
    /// Validates every name first (nothing changes on error), ignores names
    /// already present, keeps insertion order. Returns the names actually added.
    pub fn add(&mut self, names: &[String]) -> Result<Vec<String>, ConfigError> {
        // Validate everything first so that an error leaves the list untouched.
        for name in names {
            crate::config::validate_apt_package(name)?;
        }
        let mut added = Vec::new();
        for name in names {
            if !self.apt.contains(name) {
                self.apt.push(name.clone());
                added.push(name.clone());
            }
        }
        Ok(added)
    }

    /// Returns the names actually removed (unknown names are ignored).
    pub fn remove(&mut self, names: &[String]) -> Vec<String> {
        let removed: Vec<String> = names
            .iter()
            .filter(|n| self.apt.contains(n))
            .cloned()
            .collect();
        self.apt.retain(|a| !removed.contains(a));
        removed
    }
}

pub fn load(path: &Path) -> Result<ToolsFile, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let tools: ToolsFile =
        serde_json::from_str(&text).map_err(|e| ConfigError::Parse(e.to_string()))?;
    // A hand-edited tools.json must not bypass the injection check (§8.8).
    for name in &tools.apt {
        crate::config::validate_apt_package(name)?;
    }
    Ok(tools)
}

pub fn save(path: &Path, tools: &ToolsFile) -> Result<(), ConfigError> {
    let mut text =
        serde_json::to_string_pretty(tools).map_err(|e| ConfigError::Parse(e.to_string()))?;
    text.push('\n');
    crate::fsutil::write_atomic(path, text.as_bytes()).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })
}

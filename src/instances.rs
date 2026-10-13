//! Instance records in `stateRoot/instances/<id>.json` (§8.3, §8.4).

use crate::paths::Paths;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstanceRecord {
    /// 8 lowercase hex characters.
    pub id: String,
    pub agent: String,
    pub project: PathBuf,
    pub release: String,
    pub image: String,
    pub container_name: String,
    pub relay_pid: u32,
    /// Output of `ps -o lstart= -p <pid>` captured at launch (PID reuse guard).
    pub relay_started_at: String,
    /// RFC 3339 UTC.
    pub created_at: String,
}

/// State shown by `zevbox status --json` (field `state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstanceState {
    Running,
    Orphan,
    Unregistered,
}

#[derive(Debug, thiserror::Error)]
pub enum InstanceError {
    #[error("instance store error at {path}: {reason}")]
    Store { path: PathBuf, reason: String },
    #[error("not implemented yet: {0}")]
    NotImplemented(&'static str),
}

pub fn write(paths: &Paths, record: &InstanceRecord) -> Result<(), InstanceError> {
    let _ = (paths, record);
    Err(InstanceError::NotImplemented("instances::write"))
}

/// Removing a missing record is not an error.
pub fn remove(paths: &Paths, id: &str) -> Result<(), InstanceError> {
    let _ = (paths, id);
    Err(InstanceError::NotImplemented("instances::remove"))
}

/// Unreadable files are skipped and logged with `tracing::warn!`, never fatal.
pub fn list(paths: &Paths) -> Result<Vec<InstanceRecord>, InstanceError> {
    let _ = paths;
    Err(InstanceError::NotImplemented("instances::list"))
}

/// §8.4: the relay process is gone (ESRCH) or its start time differs.
pub fn is_orphan(record: &InstanceRecord) -> bool {
    let _ = record;
    false
}

/// Start time of a live process as printed by `ps -o lstart= -p <pid>` (trimmed),
/// `None` if the process does not exist.
pub fn process_start_time(pid: u32) -> Option<String> {
    let _ = pid;
    None
}

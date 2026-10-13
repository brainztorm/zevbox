//! Instance records in `stateRoot/instances/<id>.json` (§8.3, §8.4).

use crate::fsutil::{ensure_private_dir, write_atomic};
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
}

pub fn write(paths: &Paths, record: &InstanceRecord) -> Result<(), InstanceError> {
    ensure_private_dir(&paths.instances_dir()).map_err(|e| InstanceError::Store {
        path: paths.instances_dir(),
        reason: e.to_string(),
    })?;
    let path = paths.instance_file(&record.id);
    let bytes = serde_json::to_vec_pretty(record).map_err(|e| InstanceError::Store {
        path: path.clone(),
        reason: e.to_string(),
    })?;
    write_atomic(&path, &bytes).map_err(|e| InstanceError::Store {
        path,
        reason: e.to_string(),
    })
}

/// Removing a missing record is not an error.
pub fn remove(paths: &Paths, id: &str) -> Result<(), InstanceError> {
    let path = paths.instance_file(id);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(InstanceError::Store {
            path,
            reason: e.to_string(),
        }),
    }
}

/// Unreadable files are skipped and logged with `tracing::warn!`, never fatal.
pub fn list(paths: &Paths) -> Result<Vec<InstanceRecord>, InstanceError> {
    let dir = paths.instances_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(InstanceError::Store {
                path: dir,
                reason: e.to_string(),
            })
        }
    };
    let mut records = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("failed to read directory entry: {e}");
                continue;
            }
        };
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
            match std::fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<InstanceRecord>(&content) {
                    Ok(rec) => records.push(rec),
                    Err(e) => {
                        tracing::warn!("failed to parse instance record {}: {e}", path.display())
                    }
                },
                Err(e) => tracing::warn!("failed to read instance record {}: {e}", path.display()),
            }
        }
    }
    // Sort records by creation or id for stability
    records.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(records)
}

/// §8.4: the relay process is gone (ESRCH) or its start time differs.
pub fn is_orphan(record: &InstanceRecord) -> bool {
    let pid = record.relay_pid;
    // Check with kill(pid, 0)
    let pid_obj = nix::unistd::Pid::from_raw(pid as i32);
    if let Err(nix::errno::Errno::ESRCH) = nix::sys::signal::kill(pid_obj, None) {
        return true;
    }
    match process_start_time(pid) {
        Some(start_time) => start_time != record.relay_started_at,
        None => true,
    }
}

/// Start time of a live process as printed by `ps -o lstart= -p <pid>` (trimmed),
/// `None` if the process does not exist.
pub fn process_start_time(pid: u32) -> Option<String> {
    let out = std::process::Command::new("ps")
        .args(["-o", "lstart=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

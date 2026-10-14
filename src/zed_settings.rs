//! Managed `agent_servers` entries in Zed's JSONC settings (§8.9).
//! Edit through `jsonc_parser::cst` only: never serde_json, never string hacks.
//!
//! WORKED EXAMPLE (step 8, complete). The CST keeps every byte it does not
//! touch (comments, trailing commas, formatting), which is what makes the
//! user's settings safe to edit.

use crate::agents::AgentSpec;
use crate::fsutil::{sha256_hex, write_atomic};
use jsonc_parser::cst::{CstInputValue, CstRootNode};
use jsonc_parser::ParseOptions;
use std::path::{Path, PathBuf};

/// A key is managed by zevbox iff it starts with this prefix.
pub const MANAGED_PREFIX: &str = "zevbox · ";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedEntry {
    /// e.g. "zevbox · Claude".
    pub name: String,
    /// Absolute path of the installed binary.
    pub command: String,
    /// ["launch", <agent>]: the project is the working directory Zed starts
    /// the agent in (its first worktree), checked against `config.projects`
    /// by `launch` (I2).
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallOutcome {
    Unchanged,
    Updated { backup: PathBuf },
}

#[derive(Debug, thiserror::Error)]
pub enum ZedSettingsError {
    #[error("Zed settings are not valid JSONC: {0}")]
    InvalidJsonc(String),
    #[error("Zed settings root is not an object")]
    RootNotObject,
    #[error("'agent_servers' is not an object")]
    AgentServersNotObject,
    #[error("settings changed during update, retry")]
    ConcurrentModification,
    #[error("I/O error on {path}: {reason}")]
    Io { path: PathBuf, reason: String },
}

/// One entry per agent, agents in the given order, independent of the
/// allowed projects (Zed's `agent_servers` is a global setting with no
/// per-project filtering, so one entry per project would show them all in
/// every project). Name: `zevbox · <display>`.
pub fn entries_for(agents: &[AgentSpec], command: &str) -> Vec<ManagedEntry> {
    agents
        .iter()
        .map(|agent| ManagedEntry {
            name: format!("{MANAGED_PREFIX}{}", agent.display),
            command: command.to_string(),
            args: vec!["launch".to_string(), agent.id.to_string()],
        })
        .collect()
}

/// The exact value written for an entry (also used to detect "already up to date").
fn entry_json(entry: &ManagedEntry) -> serde_json::Value {
    serde_json::json!({
        "type": "custom",
        "command": entry.command,
        "args": entry.args,
        "env": {}
    })
}

fn entry_input(entry: &ManagedEntry) -> CstInputValue {
    CstInputValue::Object(vec![
        ("type".into(), CstInputValue::String("custom".into())),
        (
            "command".into(),
            CstInputValue::String(entry.command.clone()),
        ),
        (
            "args".into(),
            CstInputValue::Array(
                entry
                    .args
                    .iter()
                    .cloned()
                    .map(CstInputValue::String)
                    .collect(),
            ),
        ),
        ("env".into(), CstInputValue::Object(Vec::new())),
    ])
}

fn parse(text: &str) -> Result<CstRootNode, ZedSettingsError> {
    CstRootNode::parse(text, &ParseOptions::default())
        .map_err(|e| ZedSettingsError::InvalidJsonc(e.to_string()))
}

/// Pure text transform. Removes every managed key not in `entries`, updates or
/// appends the others (`type`, `command`, `args`, `env: {}`), creates
/// `agent_servers` if missing, never touches unmanaged keys. Empty `original`
/// (missing file) → a new document. Output must re-parse.
pub fn apply_entries(original: &str, entries: &[ManagedEntry]) -> Result<String, ZedSettingsError> {
    let source = if original.trim().is_empty() {
        "{}\n"
    } else {
        original
    };
    let root = parse(source)?;
    let settings = match root.value() {
        None => root.object_value_or_set(),
        Some(_) => root.object_value().ok_or(ZedSettingsError::RootNotObject)?,
    };
    let servers = match settings.get("agent_servers") {
        Some(prop) => prop
            .object_value()
            .ok_or(ZedSettingsError::AgentServersNotObject)?,
        // Nothing to add and nothing to remove: leave the file byte-identical.
        None if entries.is_empty() => return Ok(original.to_string()),
        None => settings.object_value_or_set("agent_servers"),
    };
    // Existing managed keys: keep if identical (byte idempotence), update, or remove.
    for prop in servers.properties() {
        let Some(name) = prop.decoded_name() else {
            continue;
        };
        if !name.starts_with(MANAGED_PREFIX) {
            continue; // F10: never touch unmanaged entries such as `claude-acp`.
        }
        match entries.iter().find(|e| e.name == name) {
            Some(entry) => {
                let current = prop.value().and_then(|v| v.to_serde_value());
                if current.as_ref() != Some(&entry_json(entry)) {
                    prop.set_value(entry_input(entry));
                }
            }
            None => prop.remove(),
        }
    }
    for entry in entries {
        if servers.get(&entry.name).is_none() {
            servers.append(&entry.name, entry_input(entry));
        }
    }
    let output = root.to_string();
    parse(&output)?; // never write something Zed could not read
    Ok(output)
}

fn io_error(path: &Path, e: std::io::Error) -> ZedSettingsError {
    ZedSettingsError::Io {
        path: path.to_path_buf(),
        reason: e.to_string(),
    }
}

/// Current file content; a missing file reads as "".
fn read_or_empty(path: &Path) -> Result<String, ZedSettingsError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(io_error(path, e)),
    }
}

/// Writes `new_text` atomically, preserving the file mode, only if the current
/// file hash equals `expected_sha256` (missing file ⇔ expected hash of "").
pub fn write_if_unchanged(
    path: &Path,
    expected_sha256: &str,
    new_text: &str,
) -> Result<(), ZedSettingsError> {
    // Zed (or the user) may have saved the file since we read it.
    if sha256_hex(read_or_empty(path)?.as_bytes()) != expected_sha256 {
        return Err(ZedSettingsError::ConcurrentModification);
    }
    write_atomic(path, new_text.as_bytes()).map_err(|e| io_error(path, e))
}

/// Full flow of §8.9: read, hash, apply, unchanged → `Unchanged` (no backup),
/// else backup `zed-settings-<UTC YYYYMMDD-HHMMSS>-<sha8>.json` into
/// `backups_dir`, then `write_if_unchanged`.
pub fn install(
    settings: &Path,
    backups_dir: &Path,
    entries: &[ManagedEntry],
) -> Result<InstallOutcome, ZedSettingsError> {
    let original = read_or_empty(settings)?;
    let sha = sha256_hex(original.as_bytes());
    let updated = apply_entries(&original, entries)?;
    if updated == original {
        return Ok(InstallOutcome::Unchanged);
    }
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let short = sha.get(..8).unwrap_or(&sha);
    let backup = backups_dir.join(format!("zed-settings-{stamp}-{short}.json"));
    write_atomic(&backup, original.as_bytes()).map_err(|e| io_error(&backup, e))?;
    write_if_unchanged(settings, &sha, &updated)?;
    Ok(InstallOutcome::Updated { backup })
}

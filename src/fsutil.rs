//! Small filesystem helpers used by every command (§6.2 rule 3).
//!
//! WORKED EXAMPLE (step 3): this file shows the expected style — std::fs is fine
//! outside the relay loop, errors are propagated with `?`, no unwrap/expect.

use sha2::{Digest, Sha256};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Mode given to files created by zevbox (state may contain secrets).
const NEW_FILE_MODE: u32 = 0o600;
const PRIVATE_DIR_MODE: u32 = 0o700;

/// Atomic write: temp file in the same directory → write_all → sync_all → persist.
/// If `path` already exists, its permission bits are preserved; otherwise the
/// new file gets mode 0600.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().ok_or_else(|| {
        std::io::Error::other(format!("{} has no parent directory", path.display()))
    })?;
    let mode = match std::fs::metadata(path) {
        Ok(meta) => meta.permissions().mode() & 0o777,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => NEW_FILE_MODE,
        Err(e) => return Err(e),
    };
    // Same directory, so the final rename is atomic (same filesystem).
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    tmp.as_file()
        .set_permissions(std::fs::Permissions::from_mode(mode))?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

/// Lowercase hex SHA-256.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Creates the directory (and parents) if needed and sets mode 0700.
pub fn ensure_private_dir(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(PRIVATE_DIR_MODE))
}

/// Keeps the `keep` most recently modified files of `dir` whose name ends with
/// `suffix`, deletes the others. Returns how many were deleted.
pub fn prune_oldest(dir: &Path, suffix: &str, keep: usize) -> std::io::Result<usize> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_file() && entry.file_name().to_string_lossy().ends_with(suffix) {
            files.push((meta.modified()?, entry.path()));
        }
    }
    // Oldest first: everything before the last `keep` entries goes.
    files.sort();
    let to_delete = files.len().saturating_sub(keep);
    for (_, path) in files.iter().take(to_delete) {
        std::fs::remove_file(path)?;
    }
    Ok(to_delete)
}

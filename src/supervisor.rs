//! `zevbox launch`: preflight, container start, relay, bounded shutdown (§8.3–§8.5).

use crate::paths::Paths;
use std::path::Path;

/// Exit codes of the whole CLI (§8.1).
pub const EXIT_OK: u8 = 0;
pub const EXIT_ERROR: u8 = 1;
pub const EXIT_USAGE: u8 = 2;
pub const EXIT_PRECONDITION: u8 = 3;

/// Full `launch` flow. Uses the process stdin/stdout for ACP. Returns the exit code.
/// On preflight failure: answer Zed's first request with the §8.5 error, exit 3.
pub async fn launch(paths: &Paths, agent: &str, project: &Path) -> u8 {
    let _ = (paths, agent, project);
    EXIT_ERROR
}

/// Removes orphan instances and their verified containers (§8.4).
/// Returns the ids cleaned. Never fails the caller: errors are logged.
pub async fn clean_orphans(paths: &Paths) -> Vec<String> {
    let _ = paths;
    Vec::new()
}

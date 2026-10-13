//! The only module allowed to write to stdout/stderr for human-facing commands (§6.1, I6).
//! Never call these functions from `launch` (stdout belongs to ACP there).

use std::io::Write;

/// Writes one line to stdout. Errors (closed pipe) are ignored on purpose.
pub fn out(line: &str) {
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{line}");
}

/// Writes one line to stderr. Errors are ignored on purpose.
pub fn err(line: &str) {
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "{line}");
}

//! The only module allowed to write to stdout/stderr for human-facing commands (§6.1, I6).
//! Never call these functions from `launch` (stdout belongs to ACP there).

use std::io::{IsTerminal, Write};

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

/// Writes a prompt to stdout without a newline, then flushes.
pub fn out_inline(text: &str) {
    let mut stdout = std::io::stdout().lock();
    let _ = write!(stdout, "{text}");
    let _ = stdout.flush();
}

/// True if stdout is a terminal (`zevbox console` refuses to start otherwise).
pub fn stdout_is_terminal() -> bool {
    std::io::stdout().is_terminal()
}

/// The stdout handle `zevbox console` draws its full screen through (ratatui
/// backend). Only the console uses it, and only on a terminal.
pub fn terminal_out() -> std::io::Stdout {
    std::io::stdout()
}

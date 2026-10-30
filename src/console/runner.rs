//! Child processes of `zevbox console` (docs/SPEC_console.md §2, §7). Every action
//! is the current binary run with arguments, one by one: no shell (I7), no
//! reimplementation. The oMLX key is written on the child's stdin, never in argv
//! or the environment (I9).
//!
//! Non-interactive runs are read line by line from two reader threads (stdout
//! and stderr, in arrival order) and polled by the main loop; interactive runs
//! inherit the real terminal.

use super::app::{LineKind, Outcome};
use super::menu::Invocation;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

/// §7: SIGTERM, then SIGKILL after this delay if the command still runs.
pub const KILL_AFTER: Duration = Duration::from_secs(5);
/// Output still in the pipes when the process exits is read for at most this long
/// (a grandchild that keeps a pipe open must not hang the console).
const DRAIN_AFTER_EXIT: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunEvent {
    /// A command starts: `zevbox …` as typed in a terminal.
    Started(String),
    Line(LineKind, String),
    /// The whole job ended.
    Finished(Outcome),
}

/// `Command` for a non-interactive run: stdout/stderr piped, stdin piped only
/// for a secret (else /dev/null, so no command ever waits for the keyboard).
/// Its own process group, so an interrupt reaches what it started too
/// (`container build`, `container stop`…).
pub fn command(exe: &Path, inv: &Invocation) -> Command {
    let mut cmd = Command::new(exe);
    cmd.process_group(0);
    cmd.args(&inv.args)
        .stdin(if inv.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

/// `Command` for an interactive run: the real terminal (§7).
pub fn interactive_command(exe: &Path, inv: &Invocation) -> Command {
    let mut cmd = Command::new(exe);
    cmd.args(&inv.args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    cmd
}

/// Runs an interactive invocation to its end on the inherited terminal.
pub fn run_interactive(exe: &Path, inv: &Invocation) -> std::io::Result<ExitStatus> {
    interactive_command(exe, inv).status()
}

enum Msg {
    Line(LineKind, String),
    Eof,
}

struct Proc {
    child: Child,
    rx: Receiver<Msg>,
    open_pipes: usize,
    exited: Option<(ExitStatus, Instant)>,
    term_sent: Option<Instant>,
}

/// The invocations of one job, run one after the other.
pub struct JobRun {
    exe: PathBuf,
    queue: VecDeque<Invocation>,
    current: Option<Proc>,
    /// First failure (exit code, signal or spawn error).
    failure: Option<Outcome>,
    /// « Interrompre » was confirmed: the job ends as interrupted, whatever the
    /// exit code, even between two steps.
    interrupted: bool,
    done: bool,
}

impl JobRun {
    pub fn new(exe: PathBuf, invocations: Vec<Invocation>) -> Self {
        Self {
            exe,
            queue: invocations.into(),
            current: None,
            failure: None,
            interrupted: false,
            done: false,
        }
    }

    pub fn is_done(&self) -> bool {
        self.done
    }

    /// Advances the job without blocking; returns what happened since the last call.
    pub fn poll(&mut self) -> Vec<RunEvent> {
        let mut events = Vec::new();
        if self.done {
            return events;
        }
        if self.current.is_none() {
            match self.queue.pop_front() {
                Some(inv) => {
                    events.push(RunEvent::Started(inv.display()));
                    match spawn(&self.exe, &inv) {
                        Ok(proc) => self.current = Some(proc),
                        Err(e) => {
                            events.push(RunEvent::Line(
                                LineKind::Err,
                                format!("cannot start {}: {e}", self.exe.display()),
                            ));
                            self.failure.get_or_insert(Outcome::CannotStart);
                            self.queue.clear();
                        }
                    }
                }
                None => {
                    self.done = true;
                    let outcome = if self.interrupted {
                        Outcome::Interrupted
                    } else {
                        self.failure.unwrap_or(Outcome::Success)
                    };
                    events.push(RunEvent::Finished(outcome));
                    return events;
                }
            }
        }
        let Some(proc) = self.current.as_mut() else {
            return events;
        };
        while let Ok(msg) = proc.rx.try_recv() {
            match msg {
                Msg::Line(kind, text) => events.push(RunEvent::Line(kind, text)),
                Msg::Eof => proc.open_pipes = proc.open_pipes.saturating_sub(1),
            }
        }
        if proc.exited.is_none() {
            match proc.child.try_wait() {
                Ok(Some(status)) => proc.exited = Some((status, Instant::now())),
                Ok(None) => {
                    if proc.term_sent.is_some_and(|t| t.elapsed() >= KILL_AFTER) {
                        signal_group(&proc.child, nix::sys::signal::Signal::SIGKILL);
                    }
                }
                Err(e) => {
                    events.push(RunEvent::Line(LineKind::Err, format!("wait failed: {e}")));
                    signal_group(&proc.child, nix::sys::signal::Signal::SIGKILL);
                }
            }
        }
        if let Some((status, at)) = proc.exited {
            if proc.open_pipes == 0 || at.elapsed() >= DRAIN_AFTER_EXIT {
                if !status.success() {
                    self.failure.get_or_insert(Outcome::Failed(status.code()));
                }
                self.current = None;
            }
        }
        events
    }

    /// §7: SIGTERM now; [`JobRun::poll`] sends SIGKILL after [`KILL_AFTER`].
    /// The rest of the job is dropped.
    pub fn interrupt(&mut self) {
        self.interrupted = true;
        self.queue.clear();
        if let Some(proc) = self.current.as_mut() {
            if proc.exited.is_none() && proc.term_sent.is_none() {
                signal_group(&proc.child, nix::sys::signal::Signal::SIGTERM);
                proc.term_sent = Some(Instant::now());
            }
        }
    }

    /// Ends the running command before the console exits: SIGTERM, up to
    /// [`KILL_AFTER`] to stop, then SIGKILL. Blocking.
    pub fn shutdown(&mut self) {
        self.queue.clear();
        let Some(mut proc) = self.current.take() else {
            return;
        };
        if proc.exited.is_some() || matches!(proc.child.try_wait(), Ok(Some(_))) {
            return;
        }
        let since = *proc.term_sent.get_or_insert_with(|| {
            signal_group(&proc.child, nix::sys::signal::Signal::SIGTERM);
            Instant::now()
        });
        while since.elapsed() < KILL_AFTER {
            if !matches!(proc.child.try_wait(), Ok(None)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        signal_group(&proc.child, nix::sys::signal::Signal::SIGKILL);
        let _ = proc.child.wait();
    }
}

impl Drop for JobRun {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Signals the child's whole process group (its id = the child's pid, see
/// [`command`]): the command and everything it started.
fn signal_group(child: &Child, signal: nix::sys::signal::Signal) {
    if let Ok(pid) = i32::try_from(child.id()) {
        let _ = nix::sys::signal::killpg(nix::unistd::Pid::from_raw(pid), signal);
    }
}

fn spawn(exe: &Path, inv: &Invocation) -> std::io::Result<Proc> {
    let mut child = command(exe, inv).spawn()?;
    if let (Some(secret), Some(mut stdin)) = (&inv.stdin, child.stdin.take()) {
        // One short line: fits in the pipe buffer, the write cannot block. Dropping
        // `stdin` closes it, so the child sees the end of input.
        if let Err(e) = stdin.write_all(secret.expose().as_bytes()) {
            signal_group(&child, nix::sys::signal::Signal::SIGKILL);
            let _ = child.wait();
            return Err(e);
        }
    }
    let (tx, rx) = mpsc::channel();
    let mut open_pipes = 0;
    if let Some(out) = child.stdout.take() {
        reader(out, LineKind::Out, tx.clone());
        open_pipes += 1;
    }
    if let Some(err) = child.stderr.take() {
        reader(err, LineKind::Err, tx);
        open_pipes += 1;
    }
    Ok(Proc {
        child,
        rx,
        open_pipes,
        exited: None,
        term_sent: None,
    })
}

fn reader(pipe: impl Read + Send + 'static, kind: LineKind, tx: Sender<Msg>) {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(pipe);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if tx.send(Msg::Line(kind, clean_line(&buf))).is_err() {
                        return;
                    }
                }
            }
        }
        let _ = tx.send(Msg::Eof);
    });
}

/// Makes one output line safe to draw: invalid UTF-8 replaced, only the last
/// `\r` segment kept (progress bars), escape sequences and control characters
/// removed (they would break the full-screen display), tabs expanded.
pub fn clean_line(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw);
    let text = text.trim_end_matches(['\n', '\r']);
    let text = text
        .rsplit('\r')
        .find(|segment| !segment.is_empty())
        .unwrap_or("");
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => match chars.peek() {
                // CSI: ESC [ params… final byte in @..~
                Some('[') => {
                    chars.next();
                    for c in chars.by_ref() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                // OSC: ESC ] … BEL (or ESC \)
                Some(']') => {
                    chars.next();
                    while let Some(c) = chars.next() {
                        if c == '\u{7}' || (c == '\u{1b}' && chars.peek() == Some(&'\\')) {
                            if c == '\u{1b}' {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                _ => {
                    chars.next();
                }
            },
            '\t' => out.push_str("    "),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::menu::{Action, Params, Secret};

    #[test]
    fn omlx_command_has_no_key_in_args_or_env() {
        let p = Params {
            url: Some("http://10.0.0.2:8000".to_string()),
            key: Some(Secret::new("sk-very-secret".to_string())),
            model: Some("m".to_string()),
            ..Params::default()
        };
        let inv = &Action::AuthOmlx.invocations(&p)[0];
        let cmd = command(Path::new("/usr/local/bin/zevbox"), inv);
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                "auth",
                "omlx",
                "--url",
                "http://10.0.0.2:8000",
                "--model",
                "m"
            ]
        );
        assert!(!args.iter().any(|a| a.contains("sk-very-secret")));
        assert_eq!(cmd.get_envs().count(), 0, "no variable is added");
        assert_eq!(cmd.get_program(), "/usr/local/bin/zevbox", "no shell (I7)");
    }

    #[test]
    fn interactive_command_runs_the_binary_directly() {
        let inv = &Action::Setup.invocations(&Params::default())[0];
        let cmd = interactive_command(Path::new("/x/zevbox"), inv);
        assert_eq!(cmd.get_program(), "/x/zevbox");
        assert_eq!(cmd.get_args().collect::<Vec<_>>(), ["setup"]);
    }

    #[test]
    fn clean_line_strips_what_would_break_the_screen() {
        assert_eq!(clean_line(b"hello\n"), "hello");
        assert_eq!(clean_line(b"10%\r50%\r100%\r\n"), "100%");
        assert_eq!(clean_line(b"\x1b[1;31mred\x1b[0m ok"), "red ok");
        assert_eq!(clean_line(b"\x1b]0;title\x07after"), "after");
        assert_eq!(clean_line(b"a\tb\x08c"), "a    bc");
        assert_eq!(clean_line(b"bad \xff utf8"), "bad \u{fffd} utf8");
    }

    fn run_to_end(job: &mut JobRun) -> Vec<RunEvent> {
        let mut all = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !job.is_done() && Instant::now() < deadline {
            all.extend(job.poll());
            std::thread::sleep(Duration::from_millis(10));
        }
        all
    }

    fn inv(args: &[&str], stdin: Option<&str>) -> Invocation {
        Invocation {
            args: args.iter().map(|s| s.to_string()).collect(),
            stdin: stdin.map(|s| Secret::new(s.to_string())),
            interactive: false,
        }
    }

    #[test]
    fn job_streams_stdout_and_feeds_stdin() {
        // /bin/cat echoes its stdin: proves the secret goes through stdin only.
        let mut job = JobRun::new(PathBuf::from("/bin/cat"), vec![inv(&[], Some("line\n"))]);
        let events = run_to_end(&mut job);
        assert!(events.contains(&RunEvent::Line(LineKind::Out, "line".to_string())));
        assert_eq!(events.last(), Some(&RunEvent::Finished(Outcome::Success)));
    }

    #[test]
    fn job_reports_the_first_failure_and_runs_every_invocation() {
        let mut job = JobRun::new(
            PathBuf::from("/bin/ls"),
            vec![inv(&["/nonexistent-zevbox-path"], None), inv(&["/"], None)],
        );
        let events = run_to_end(&mut job);
        let started = events
            .iter()
            .filter(|e| matches!(e, RunEvent::Started(_)))
            .count();
        assert_eq!(started, 2);
        assert!(events
            .iter()
            .any(|e| matches!(e, RunEvent::Line(LineKind::Err, _))));
        assert!(matches!(
            events.last(),
            Some(RunEvent::Finished(Outcome::Failed(Some(c)))) if *c != 0
        ));
    }

    #[test]
    fn interrupt_sends_sigterm() {
        let mut job = JobRun::new(PathBuf::from("/bin/sleep"), vec![inv(&["30"], None)]);
        let _ = job.poll();
        let start = Instant::now();
        job.interrupt();
        let events = run_to_end(&mut job);
        assert!(start.elapsed() < KILL_AFTER, "SIGTERM was enough");
        assert_eq!(
            events.last(),
            Some(&RunEvent::Finished(Outcome::Interrupted))
        );
    }

    #[test]
    fn interrupt_reaches_grandchildren() {
        // /usr/bin/time starts /bin/sleep as its own child: the grandchild of the
        // console. Both must be gone once the job has ended.
        let mut job = JobRun::new(
            PathBuf::from("/usr/bin/time"),
            vec![inv(&["/bin/sleep", "30"], None)],
        );
        let _ = job.poll();
        let group = job
            .current
            .as_ref()
            .map(|p| p.child.id())
            .and_then(|id| i32::try_from(id).ok())
            .map(nix::unistd::Pid::from_raw)
            .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        job.interrupt();
        let events = run_to_end(&mut job);
        assert_eq!(
            events.last(),
            Some(&RunEvent::Finished(Outcome::Interrupted))
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        while nix::sys::signal::killpg(group, None).is_ok() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(
            nix::sys::signal::killpg(group, None),
            Err(nix::errno::Errno::ESRCH),
            "a process of the group survived"
        );
    }

    #[test]
    fn interrupt_between_two_steps_is_not_a_success() {
        let mut job = JobRun::new(
            PathBuf::from("/usr/bin/true"),
            vec![inv(&[], None), inv(&[], None)],
        );
        // Run the first step to its end, then interrupt before the second starts.
        let _ = job.poll();
        while job.current.is_some() {
            let _ = job.poll();
            std::thread::sleep(Duration::from_millis(10));
        }
        job.interrupt();
        let events = run_to_end(&mut job);
        assert!(!events.iter().any(|e| matches!(e, RunEvent::Started(_))));
        assert_eq!(
            events.last(),
            Some(&RunEvent::Finished(Outcome::Interrupted))
        );
    }

    #[test]
    fn missing_binary_is_a_failure_not_a_crash() {
        let mut job = JobRun::new(
            PathBuf::from("/nonexistent/zevbox"),
            vec![inv(&["status"], None)],
        );
        let events = run_to_end(&mut job);
        assert_eq!(
            events.last(),
            Some(&RunEvent::Finished(Outcome::CannotStart))
        );
        assert_eq!(
            Outcome::CannotStart.label(crate::console::i18n::Lang::Fr),
            "✗ échec (impossible de lancer la commande)"
        );
    }
}

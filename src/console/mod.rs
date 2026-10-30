//! `zevbox console` (docs/SPEC_console.md): a full-screen keyboard menu over the
//! other commands. It never reimplements one: every action runs the current
//! binary with arguments (§2), so the command line stays the only source of
//! truth. It reads the state for its banner and pickers, and never writes it.
//!
//! Layout: [`menu`] (pure tree and commands), [`state`] (read-only snapshot),
//! [`app`] (pure state machine), `ui` (drawing), `text` (column widths),
//! [`runner`] (child processes).
//! This file owns the terminal: raw mode, alternate screen and hidden cursor are
//! always restored, on quit, on error, on panic (unwinding drops [`Tui`]) and
//! around interactive commands (§7).

pub mod app;
pub mod i18n;
pub mod menu;
pub mod runner;
pub mod state;
mod text;
mod ui;

use crate::paths::Paths;
use crate::supervisor::{EXIT_OK, EXIT_USAGE};
use app::{App, Effect, Job, LineKind, Outcome};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use i18n::{Lang, Msg};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use runner::{JobRun, RunEvent};
use state::Snapshot;
use std::io::IsTerminal;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Entry point of `zevbox console`. Works before `setup` (no config needed):
/// the banner then says so and « First-time setup » is one Enter away.
/// `lang`: `--lang`, else the locale ([`Lang::from_env`]).
pub async fn run(lang: Option<Lang>) -> anyhow::Result<u8> {
    let lang = lang.unwrap_or_else(Lang::from_env);
    if !std::io::stdin().is_terminal() || !crate::ui::stdout_is_terminal() {
        crate::ui::err("zevbox console needs an interactive terminal.");
        return Ok(EXIT_USAGE);
    }
    let paths = Paths::from_env()?;
    let exe = std::env::current_exe()?;
    let quit = watch_signals()?;
    tokio::task::spawn_blocking(move || run_blocking(&paths, &exe, &quit, lang))
        .await
        .map_err(|e| anyhow::anyhow!("the console stopped unexpectedly: {e}"))?
}

/// SIGINT is swallowed (Ctrl-C during an interactive command belongs to that
/// command; in raw mode it is a key). SIGTERM and SIGHUP ask the loop to quit,
/// so the terminal is restored and the running command stopped.
fn watch_signals() -> std::io::Result<Arc<AtomicBool>> {
    use tokio::signal::unix::{signal, SignalKind};
    let quit = Arc::new(AtomicBool::new(false));
    for (kind, quits) in [
        (SignalKind::interrupt(), false),
        (SignalKind::terminate(), true),
        (SignalKind::hangup(), true),
    ] {
        let mut stream = signal(kind)?;
        let flag = Arc::clone(&quit);
        tokio::spawn(async move {
            while stream.recv().await.is_some() {
                if quits {
                    flag.store(true, Ordering::SeqCst);
                }
            }
        });
    }
    Ok(quit)
}

type Backend = CrosstermBackend<std::io::Stdout>;

/// The full-screen terminal. Dropping it restores the user's terminal.
struct Tui {
    terminal: Terminal<Backend>,
    /// Raw mode and alternate screen are on.
    active: bool,
}

impl Tui {
    fn enter() -> std::io::Result<Self> {
        let terminal = Terminal::new(CrosstermBackend::new(crate::ui::terminal_out()))?;
        let mut tui = Self {
            terminal,
            active: false,
        };
        tui.resume()?;
        Ok(tui)
    }

    fn resume(&mut self) -> std::io::Result<()> {
        // `active` first: a failure half-way is still undone by `Drop`.
        self.active = true;
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(
            self.terminal.backend_mut(),
            EnterAlternateScreen,
            crossterm::cursor::Hide,
            crossterm::terminal::Clear(crossterm::terminal::ClearType::All)
        )?;
        // Forget what was drawn before: one empty frame makes ratatui's previous
        // buffer blank, so the next draw repaints everything. (`Terminal::clear`
        // would ask the terminal for the cursor position, which some do not answer.)
        self.terminal.draw(|_| {}).map(|_| ())
    }

    fn suspend(&mut self) -> std::io::Result<()> {
        self.active = false;
        let raw = crossterm::terminal::disable_raw_mode();
        crossterm::execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            crossterm::cursor::Show
        )?;
        raw
    }

    fn too_small(&self) -> bool {
        self.terminal
            .size()
            .is_ok_and(|s| s.width < ui::MIN_WIDTH || s.height < ui::MIN_HEIGHT)
    }

    fn draw(&mut self, app: &App) -> std::io::Result<()> {
        self.terminal.draw(|frame| ui::draw(frame, app)).map(|_| ())
    }
}

/// Puts the user's terminal back (no `Tui` needed: used by the panic hook).
fn restore_terminal() {
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(
        crate::ui::terminal_out(),
        LeaveAlternateScreen,
        crossterm::cursor::Show
    );
}

/// While the console runs, a panic first restores the terminal, then prints
/// its message through the previous hook (else it would be lost in the
/// alternate screen). Dropping the guard puts the default hook back.
struct PanicHook;

impl PanicHook {
    fn install() -> Self {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore_terminal();
            previous(info);
        }));
        Self
    }
}

impl Drop for PanicHook {
    fn drop(&mut self) {
        // `take_hook` must not be called while unwinding (it would abort).
        if !std::thread::panicking() {
            let _ = std::panic::take_hook();
        }
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        if self.active {
            let _ = self.suspend();
        }
    }
}

fn run_blocking(paths: &Paths, exe: &Path, quit: &AtomicBool, lang: Lang) -> anyhow::Result<u8> {
    let _hook = PanicHook::install();
    let mut tui = Tui::enter()?;
    let mut app = App::new(Snapshot::load(paths), lang);
    let mut job: Option<JobRun> = None;
    loop {
        if quit.load(Ordering::SeqCst) {
            break;
        }
        if let Some(run) = job.as_mut() {
            for ev in run.poll() {
                match ev {
                    RunEvent::Started(cmd) => app.push_line(LineKind::Info, format!("$ {cmd}")),
                    RunEvent::Line(kind, text) => app.push_line(kind, text),
                    RunEvent::Finished(outcome) => {
                        app.finished(outcome);
                        // §7: banner and lists read again after each command.
                        app.snap = Snapshot::load(paths);
                    }
                }
            }
            if run.is_done() {
                job = None;
                if app.quit_after_run {
                    break;
                }
            }
        }
        tui.draw(&app)?;
        if let Ok(size) = tui.terminal.size() {
            app.output_body = ui::output_body_size(size.width, size.height);
        }
        let wait = Duration::from_millis(if job.is_some() { 30 } else { 250 });
        if !event::poll(wait)? {
            continue;
        }
        // Resize and other events only need the redraw of the next turn.
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        // Too small: nothing is visible, so only « quit » is understood (§3).
        if tui.too_small() {
            let quit_key = key.code == KeyCode::Char('q')
                || (key.code == KeyCode::Char('c')
                    && key.modifiers.contains(KeyModifiers::CONTROL));
            if quit_key && !app.is_running() {
                break;
            }
            continue;
        }
        match app.handle_key(key) {
            Effect::None => {}
            Effect::Quit => break,
            Effect::Interrupt => {
                if let Some(run) = job.as_mut() {
                    run.interrupt();
                }
            }
            Effect::Start(j) if j.interactive => {
                run_interactive(&mut tui, exe, &j, lang)?;
                app.snap = Snapshot::load(paths);
            }
            Effect::Start(j) => job = Some(JobRun::new(exe.to_path_buf(), j.invocations)),
        }
    }
    // Stops a command still running (SIGTERM, then SIGKILL) before the screen goes.
    drop(job);
    drop(tui);
    Ok(EXIT_OK)
}

/// §7: leave the full screen, run on the real terminal, wait for Enter, redraw.
fn run_interactive(tui: &mut Tui, exe: &Path, job: &Job, lang: Lang) -> anyhow::Result<()> {
    tui.suspend()?;
    for inv in &job.invocations {
        crate::ui::out(&format!("$ {}", inv.display()));
        match runner::run_interactive(exe, inv) {
            Ok(status) if status.success() => crate::ui::out(&Outcome::Success.label(lang)),
            Ok(status) => crate::ui::err(&Outcome::Failed(status.code()).label(lang)),
            Err(e) => {
                crate::ui::err(&format!("cannot start {}: {e}", exe.display()));
                crate::ui::err(&Outcome::CannotStart.label(lang));
            }
        }
    }
    crate::ui::out_inline(lang.t(Msg::PressEnter));
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    tui.resume()?;
    Ok(())
}

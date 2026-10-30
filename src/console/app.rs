//! State machine of `zevbox console` (docs/SPEC_console.md §4, §6, §7): menu
//! navigation, pickers, text inputs (masked for the oMLX key), « Êtes-vous sûr ? »
//! and the output view. Pure: keys go in, [`Effect`]s come out; the terminal and
//! the child processes live in `mod.rs` and `runner.rs`.

use super::i18n::{Lang, Msg};
use super::menu::{
    self, Action, AgentSet, Confirmation, Entry, InstanceChoice, Invocation, Menu, PanelLine,
    Params, Secret, Step, TextField,
};
use super::state::Snapshot;
use super::text;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// What the main loop must do after a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    None,
    Quit,
    /// Run this job (non-interactive: the output view is already shown).
    Start(Job),
    /// SIGTERM the running command (SIGKILL 5 s later, §7).
    Interrupt,
}

/// One leaf with all its parameters, ready to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub title: String,
    pub invocations: Vec<Invocation>,
    pub interactive: bool,
}

impl Job {
    pub fn new(action: Action, params: &Params, lang: Lang) -> Self {
        Self {
            title: action.label(lang).to_string(),
            invocations: action.invocations(params),
            interactive: action.is_interactive(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub label: String,
    /// Empty for « la plus récente ».
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picker {
    pub title: &'static str,
    pub choices: Vec<Choice>,
    pub cursor: usize,
    /// Shown instead of the list when it is empty.
    pub empty_message: &'static str,
}

#[derive(Clone, PartialEq, Eq)]
pub struct TextInput {
    pub field: TextField,
    pub chars: Vec<char>,
    /// Insertion point, in chars.
    pub cursor: usize,
    /// Enter on an empty value is accepted.
    pub optional: bool,
    /// Why the last Enter was refused (shown under the field).
    pub error: Option<Msg>,
}

/// Masked fields show `•` only, even in `{:?}` (I9).
impl std::fmt::Debug for TextInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextInput")
            .field("field", &self.field)
            .field("value", &self.shown())
            .field("cursor", &self.cursor)
            .field("optional", &self.optional)
            .field("error", &self.error)
            .finish()
    }
}

/// The key may have been typed here: best-effort wipe, like [`Secret`].
impl Drop for TextInput {
    fn drop(&mut self) {
        self.chars.iter_mut().for_each(|c| *c = '\0');
        std::hint::black_box(&self.chars);
    }
}

impl TextInput {
    fn new(field: TextField, initial: &str, optional: bool) -> Self {
        let chars: Vec<char> = initial.chars().collect();
        Self {
            field,
            cursor: chars.len(),
            chars,
            optional,
            error: None,
        }
    }

    pub fn value(&self) -> String {
        self.chars.iter().collect()
    }

    /// What the screen shows: `•` per character for the key.
    pub fn shown(&self) -> String {
        if self.field.masked() {
            "•".repeat(self.chars.len())
        } else {
            self.value()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wizard {
    pub action: Action,
    pub step: usize,
    pub params: Params,
    pub picker: Option<Picker>,
    pub text: Option<TextInput>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Out,
    Err,
    /// Lines written by the console itself (`$ zevbox …`).
    Info,
}

/// How a whole job ended (§7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Success,
    /// Exit code of the first failed step; `None` = ended by a signal.
    Failed(Option<i32>),
    /// Stopped by « Interrompre la commande ? » (also between two steps).
    Interrupted,
    /// The binary could not be started at all.
    CannotStart,
}

impl Outcome {
    pub fn is_success(self) -> bool {
        self == Outcome::Success
    }

    /// `✓ terminé` / `✗ échec (code N)` (§7).
    pub fn label(self, lang: Lang) -> String {
        match self {
            Outcome::Success => lang.t(Msg::Done).to_string(),
            Outcome::Failed(Some(code)) => lang.f(Msg::FailedCode, &[&code.to_string()]),
            Outcome::Failed(None) => lang.t(Msg::FailedSignal).to_string(),
            Outcome::Interrupted => lang.t(Msg::Interrupted).to_string(),
            Outcome::CannotStart => lang.t(Msg::CannotStart).to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Running { interrupting: bool },
    Done(Outcome),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub title: String,
    pub lines: Vec<(LineKind, String)>,
    /// Rows scrolled up from the bottom (0 = follow the end).
    pub scroll: usize,
    pub status: RunStatus,
}

/// Lines kept in the output view (oldest dropped first).
pub const MAX_OUTPUT_LINES: usize = 5000;
const PAGE: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    Menu,
    Wizard(Box<Wizard>),
    Output(Output),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Purpose {
    Run(Job),
    Interrupt { then_quit: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmDialog {
    pub confirmation: Confirmation,
    /// Non is selected by default (§6).
    pub yes_selected: bool,
    pub purpose: Purpose,
}

impl ConfirmDialog {
    pub fn new(confirmation: Confirmation, purpose: Purpose) -> Self {
        Self {
            confirmation,
            yes_selected: false,
            purpose,
        }
    }

    /// `Some(answer)` once decided: Enter = the selected button, Échap = Non.
    pub fn key(&mut self, code: KeyCode) -> Option<bool> {
        match code {
            KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                self.yes_selected = !self.yes_selected;
                None
            }
            KeyCode::Enter => Some(self.yes_selected),
            KeyCode::Esc => Some(false),
            _ => None,
        }
    }
}

pub struct App {
    /// Interface language (`--lang`, else the locale).
    pub lang: Lang,
    pub root: Menu,
    /// Indices of the opened submenus.
    pub path: Vec<usize>,
    /// Selected entry at each level (`path.len() + 1` items).
    pub cursors: Vec<usize>,
    pub snap: Snapshot,
    pub view: View,
    pub confirm: Option<ConfirmDialog>,
    /// Quit once the running command has ended (interrupt then quit).
    pub quit_after_run: bool,
    /// Rows of the description scrolled with PgUp/PgDn (reset on every move).
    pub desc_scroll: usize,
    /// Columns and rows of the output body, set by the main loop from the
    /// terminal size (`ui::output_body_size`): scrolling stops at the first row.
    pub output_body: (usize, usize),
}

/// Panel receiving the keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The menu on the left.
    Menu,
    /// The action on the right (choice, text, output, question).
    Action,
}

fn is_ctrl_c(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c')
}

impl App {
    pub fn new(snap: Snapshot, lang: Lang) -> Self {
        Self {
            lang,
            root: menu::root(),
            path: Vec::new(),
            cursors: vec![0],
            snap,
            view: View::Menu,
            confirm: None,
            quit_after_run: false,
            desc_scroll: 0,
            output_body: (0, 0),
        }
    }

    pub fn current_menu(&self) -> &Menu {
        menu::menu_at(&self.root, &self.path).unwrap_or(&self.root)
    }

    pub fn cursor(&self) -> usize {
        self.cursors.last().copied().unwrap_or(0)
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        self.current_menu().children.get(self.cursor())
    }

    /// `zevbox › Mises à jour` (§3).
    pub fn path_title(&self) -> String {
        let mut parts = vec![self.lang.t(self.root.label)];
        let mut menu = &self.root;
        for &i in &self.path {
            if let Some(Entry::Menu(m)) = menu.children.get(i) {
                parts.push(self.lang.t(m.label));
                menu = m;
            }
        }
        parts.join(" › ")
    }

    /// Which panel receives the keys (drawn with a blue border, §3): the menu on
    /// the left, or, once an action is open (choice, text, output, question),
    /// the right panel.
    pub fn focus(&self) -> Focus {
        match self.view {
            View::Menu => Focus::Menu,
            View::Wizard(_) | View::Output(_) => Focus::Action,
        }
    }

    /// The description shown on the right (menu view, or an action being set up).
    pub fn panel_lines(&self) -> Vec<PanelLine> {
        match &self.view {
            View::Wizard(w) => w.action.panel(&w.params, &self.snap, self.lang),
            _ => match self.selected_entry() {
                Some(Entry::Menu(m)) => menu::menu_panel(m, self.lang),
                Some(Entry::Leaf(a)) => a.panel(&Params::default(), &self.snap, self.lang),
                None => Vec::new(),
            },
        }
    }

    /// Highest useful [`App::desc_scroll`] for the panel size known so far.
    pub fn desc_max_scroll(&self) -> usize {
        let (columns, rows) = self.output_body;
        let lines = self.panel_lines();
        if columns == 0 {
            return lines.len();
        }
        let total: usize = lines
            .iter()
            .map(|(_, l)| text::wrap_words(l, columns).len())
            .sum();
        // The description has the output's status row too.
        total.saturating_sub(rows + 1)
    }

    pub fn is_running(&self) -> bool {
        matches!(
            &self.view,
            View::Output(Output {
                status: RunStatus::Running { .. },
                ..
            })
        )
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Effect {
        if let Some(dialog) = self.confirm.take() {
            return self.confirm_key(dialog, key);
        }
        match std::mem::replace(&mut self.view, View::Menu) {
            View::Menu => self.menu_key(key),
            View::Wizard(w) => self.wizard_key(*w, key),
            View::Output(o) => self.output_key(o, key),
        }
    }

    fn confirm_key(&mut self, mut dialog: ConfirmDialog, key: KeyEvent) -> Effect {
        let quit_key = is_ctrl_c(&key) || key.code == KeyCode::Char('q');
        if quit_key && matches!(dialog.purpose, Purpose::Run(_)) {
            return Effect::Quit;
        }
        match dialog.key(key.code) {
            None => {
                self.confirm = Some(dialog);
                Effect::None
            }
            Some(answer) => match (dialog.purpose, answer) {
                (Purpose::Run(job), true) => self.start(job),
                (Purpose::Run(_), false) => {
                    self.view = View::Menu;
                    Effect::None
                }
                (Purpose::Interrupt { then_quit }, true) => {
                    self.quit_after_run = then_quit;
                    if let View::Output(o) = &mut self.view {
                        if let RunStatus::Running { interrupting } = &mut o.status {
                            *interrupting = true;
                        }
                    }
                    Effect::Interrupt
                }
                (Purpose::Interrupt { .. }, false) => Effect::None,
            },
        }
    }

    fn menu_key(&mut self, key: KeyEvent) -> Effect {
        let len = self.current_menu().children.len().max(1);
        let page = self.output_body.1.max(1);
        if matches!(key.code, KeyCode::PageUp | KeyCode::PageDown) {
            // PgUp/PgDn read a description longer than the panel; the menu keeps
            // the arrows.
            let max = self.desc_max_scroll();
            self.desc_scroll = if key.code == KeyCode::PageDown {
                (self.desc_scroll + page).min(max)
            } else {
                self.desc_scroll.min(max).saturating_sub(page)
            };
            return Effect::None;
        }
        self.desc_scroll = 0;
        match key.code {
            _ if is_ctrl_c(&key) => Effect::Quit,
            KeyCode::Char('q') => Effect::Quit,
            KeyCode::Up => {
                self.move_cursor(len - 1, len);
                Effect::None
            }
            KeyCode::Down => {
                self.move_cursor(1, len);
                Effect::None
            }
            KeyCode::Enter | KeyCode::Right => match self.selected_entry().cloned() {
                Some(Entry::Menu(_)) => {
                    self.path.push(self.cursor());
                    self.cursors.push(0);
                    Effect::None
                }
                Some(Entry::Leaf(action)) => self.open_action(action),
                None => Effect::None,
            },
            KeyCode::Esc | KeyCode::Left => {
                if self.path.pop().is_none() {
                    return Effect::Quit;
                }
                self.cursors.pop();
                Effect::None
            }
            _ => Effect::None,
        }
    }

    fn move_cursor(&mut self, delta: usize, len: usize) {
        if let Some(c) = self.cursors.last_mut() {
            *c = (*c + delta) % len;
        }
    }

    /// Enter on a leaf: ask its parameters, or go straight to confirm/run.
    pub fn open_action(&mut self, action: Action) -> Effect {
        let mut wizard = Wizard {
            action,
            step: 0,
            params: Params::default(),
            picker: None,
            text: None,
        };
        if action.steps().is_empty() {
            return self.finish(wizard);
        }
        self.enter_step(&mut wizard);
        self.view = View::Wizard(Box::new(wizard));
        Effect::None
    }

    /// Prepares the picker or the text input of `wizard.step`.
    fn enter_step(&self, w: &mut Wizard) {
        w.picker = None;
        w.text = None;
        match w.action.steps().get(w.step).copied() {
            Some(Step::Text(field)) => {
                let p = &w.params;
                let (initial, optional) = match field {
                    TextField::Packages => (p.packages.join(" "), false),
                    TextField::Url => (
                        p.url
                            .clone()
                            .or_else(|| self.snap.omlx_url.clone())
                            .unwrap_or_default(),
                        false,
                    ),
                    // Empty = keep the current key, only if there is one. Copied
                    // straight from the secret below (no plain `String` copy).
                    TextField::Key => (String::new(), self.snap.is_authenticated("omlx")),
                    // The current default model: an empty field would make
                    // `auth omlx` pick the smallest model again.
                    TextField::Model => (
                        p.model
                            .clone()
                            .or_else(|| self.snap.omlx_model.clone())
                            .unwrap_or_default(),
                        true,
                    ),
                    TextField::Project => (
                        p.project.clone().unwrap_or_else(|| self.snap.cwd.clone()),
                        false,
                    ),
                };
                let initial = match (field, &p.key) {
                    (TextField::Key, Some(key)) => key.expose(),
                    _ => initial.as_str(),
                };
                w.text = Some(TextInput::new(field, initial, optional));
            }
            Some(step) => w.picker = Some(self.picker_for(step)),
            None => {}
        }
    }

    pub fn picker_for(&self, step: Step) -> Picker {
        let snap = &self.snap;
        let lang = self.lang;
        let agent_choice = |a: &super::state::AgentState| Choice {
            label: match &a.version {
                Some(v) => format!("{} {v}", a.display),
                None => lang.f(Msg::AgentNotInstalled, &[a.display]),
            },
            value: a.id.to_string(),
        };
        let instance_label = |id: &str| match snap.instances.iter().find(|i| i.id == id) {
            Some(i) => format!("{} · {} · {}", i.id, i.agent, i.project),
            None => id.to_string(),
        };
        match step {
            Step::Instance {
                allow_latest: false,
            } => Picker {
                title: lang.t(Msg::PickStop),
                choices: snap
                    .instances
                    .iter()
                    .map(|i| Choice {
                        label: instance_label(&i.id),
                        value: i.id.clone(),
                    })
                    .collect(),
                cursor: 0,
                empty_message: lang.t(Msg::NoAgentRunning),
            },
            Step::Instance { allow_latest: true } => {
                let mut choices = vec![Choice {
                    label: lang.t(Msg::Latest).to_string(),
                    value: String::new(),
                }];
                let mut ids: Vec<&String> = snap.sessions.iter().collect();
                for i in &snap.instances {
                    if !ids.contains(&&i.id) {
                        ids.push(&i.id);
                    }
                }
                choices.extend(ids.into_iter().map(|id| Choice {
                    label: instance_label(id),
                    value: id.clone(),
                }));
                Picker {
                    title: lang.t(Msg::PickSession),
                    choices,
                    cursor: 0,
                    empty_message: "",
                }
            }
            Step::Agent(set) => {
                let (choices, empty_message): (Vec<Choice>, &'static str) = match set {
                    AgentSet::All => (snap.agents.iter().map(agent_choice).collect(), ""),
                    AgentSet::Installed => (
                        snap.agents
                            .iter()
                            .filter(|a| a.version.is_some())
                            .map(agent_choice)
                            .collect(),
                        lang.t(Msg::NoAgentInstalled),
                    ),
                    AgentSet::WithPrevious => (
                        snap.agents
                            .iter()
                            .filter(|a| a.has_previous)
                            .map(agent_choice)
                            .collect(),
                        lang.t(Msg::NoPrevious),
                    ),
                };
                Picker {
                    title: lang.t(Msg::PickAgent),
                    choices,
                    cursor: 0,
                    empty_message,
                }
            }
            Step::Tool => Picker {
                title: lang.t(Msg::PickTool),
                choices: snap
                    .tools
                    .iter()
                    .map(|t| Choice {
                        label: t.clone(),
                        value: t.clone(),
                    })
                    .collect(),
                cursor: 0,
                empty_message: lang.t(Msg::NoTool),
            },
            Step::Text(_) => Picker {
                title: "",
                choices: Vec::new(),
                cursor: 0,
                empty_message: "",
            },
        }
    }

    fn wizard_key(&mut self, mut w: Wizard, key: KeyEvent) -> Effect {
        if is_ctrl_c(&key) {
            return Effect::Quit;
        }
        if let Some(text) = w.text.as_mut() {
            if key.code != KeyCode::Enter {
                text.error = None;
            }
            match key.code {
                KeyCode::Esc => return self.step_back(w),
                KeyCode::Enter => {
                    // A `Secret` so the copy of a typed key is wiped too.
                    let value = Secret::new(text.value());
                    let trimmed = value.expose().trim();
                    if trimmed.is_empty() && !text.optional {
                        self.view = View::Wizard(Box::new(w));
                        return Effect::None;
                    }
                    if let Err(message) = text.field.check(trimmed) {
                        text.error = Some(message);
                        self.view = View::Wizard(Box::new(w));
                        return Effect::None;
                    }
                    let field = text.field;
                    let p = &mut w.params;
                    match field {
                        TextField::Packages => {
                            p.packages = trimmed.split_whitespace().map(str::to_string).collect()
                        }
                        TextField::Url => p.url = Some(trimmed.to_string()),
                        TextField::Key => p.key = Some(Secret::new(trimmed.to_string())),
                        TextField::Model => p.model = Some(trimmed.to_string()),
                        TextField::Project => p.project = Some(trimmed.to_string()),
                    }
                    return self.step_forward(w);
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    text.chars.insert(text.cursor, c);
                    text.cursor += 1;
                }
                KeyCode::Backspace if text.cursor > 0 => {
                    text.cursor -= 1;
                    text.chars.remove(text.cursor);
                }
                KeyCode::Delete if text.cursor < text.chars.len() => {
                    text.chars.remove(text.cursor);
                }
                KeyCode::Left => text.cursor = text.cursor.saturating_sub(1),
                KeyCode::Right => text.cursor = (text.cursor + 1).min(text.chars.len()),
                KeyCode::Home => text.cursor = 0,
                KeyCode::End => text.cursor = text.chars.len(),
                _ => {}
            }
            self.view = View::Wizard(Box::new(w));
            return Effect::None;
        }
        let Some(picker) = w.picker.as_mut() else {
            return Effect::None;
        };
        let len = picker.choices.len();
        match key.code {
            KeyCode::Char('q') => return Effect::Quit,
            KeyCode::Esc | KeyCode::Left => return self.step_back(w),
            KeyCode::Up if len > 0 => picker.cursor = (picker.cursor + len - 1) % len,
            KeyCode::Down if len > 0 => picker.cursor = (picker.cursor + 1) % len,
            KeyCode::Enter | KeyCode::Right if len == 0 => return self.step_back(w),
            KeyCode::Enter | KeyCode::Right => {
                let value = picker
                    .choices
                    .get(picker.cursor)
                    .map(|c| c.value.clone())
                    .unwrap_or_default();
                match w.action.steps().get(w.step) {
                    Some(Step::Instance { .. }) => {
                        w.params.instance = Some(if value.is_empty() {
                            InstanceChoice::Latest
                        } else {
                            InstanceChoice::Id(value)
                        })
                    }
                    Some(Step::Agent(_)) => w.params.agent = Some(value),
                    Some(Step::Tool) => w.params.tool = Some(value),
                    _ => {}
                }
                return self.step_forward(w);
            }
            _ => {}
        }
        self.view = View::Wizard(Box::new(w));
        Effect::None
    }

    fn step_back(&mut self, mut w: Wizard) -> Effect {
        if w.step == 0 {
            self.view = View::Menu;
            return Effect::None;
        }
        w.step -= 1;
        self.enter_step(&mut w);
        self.view = View::Wizard(Box::new(w));
        Effect::None
    }

    fn step_forward(&mut self, mut w: Wizard) -> Effect {
        w.step += 1;
        if w.step >= w.action.steps().len() {
            return self.finish(w);
        }
        self.enter_step(&mut w);
        self.view = View::Wizard(Box::new(w));
        Effect::None
    }

    /// Every parameter is known: confirm (§6) or run.
    fn finish(&mut self, w: Wizard) -> Effect {
        let job = Job::new(w.action, &w.params, self.lang);
        match w.action.confirmation(&w.params, &self.snap, self.lang) {
            Some(confirmation) => {
                // Keep the wizard visible behind the box (its preview is complete).
                self.view = View::Wizard(Box::new(Wizard {
                    step: w.action.steps().len(),
                    picker: None,
                    text: None,
                    ..w
                }));
                self.confirm = Some(ConfirmDialog::new(confirmation, Purpose::Run(job)));
                Effect::None
            }
            None => self.start(job),
        }
    }

    fn start(&mut self, job: Job) -> Effect {
        self.view = if job.interactive {
            View::Menu
        } else {
            View::Output(Output {
                title: job.title.clone(),
                lines: Vec::new(),
                scroll: 0,
                status: RunStatus::Running {
                    interrupting: false,
                },
            })
        };
        Effect::Start(job)
    }

    fn output_key(&mut self, mut o: Output, key: KeyEvent) -> Effect {
        let running = matches!(o.status, RunStatus::Running { .. });
        let quit = is_ctrl_c(&key) || key.code == KeyCode::Char('q');
        let max = max_scroll(&o, self.output_body);
        let page = match self.output_body.1 {
            0 => PAGE,
            rows => rows,
        };
        match key.code {
            KeyCode::Up => o.scroll = (o.scroll + 1).min(max),
            KeyCode::Down => o.scroll = o.scroll.min(max).saturating_sub(1),
            KeyCode::PageUp => o.scroll = (o.scroll + page).min(max),
            KeyCode::PageDown => o.scroll = o.scroll.min(max).saturating_sub(page),
            KeyCode::Esc | KeyCode::Left if running => {
                self.confirm = Some(interrupt_dialog(false, self.lang));
            }
            _ if quit && running => self.confirm = Some(interrupt_dialog(true, self.lang)),
            _ if quit => return Effect::Quit,
            KeyCode::Esc | KeyCode::Left | KeyCode::Enter if !running => {
                self.view = View::Menu;
                return Effect::None;
            }
            _ => {}
        }
        self.view = View::Output(o);
        Effect::None
    }

    pub fn push_line(&mut self, kind: LineKind, text: String) {
        if let View::Output(o) = &mut self.view {
            // stderr: same colour as stdout (no fourth colour, §3), marked by « ! ».
            let text = match kind {
                LineKind::Err => format!("! {text}"),
                _ => text,
            };
            o.lines.push((kind, text));
            if o.lines.len() > MAX_OUTPUT_LINES {
                o.lines.remove(0);
            }
        }
    }

    pub fn finished(&mut self, outcome: Outcome) {
        if let View::Output(o) = &mut self.view {
            o.status = RunStatus::Done(outcome);
        }
        // An interrupt question about a command that ended by itself is moot.
        if matches!(
            self.confirm,
            Some(ConfirmDialog {
                purpose: Purpose::Interrupt { .. },
                ..
            })
        ) {
            self.confirm = None;
        }
    }

    /// Bottom line (§3): where the keys go, then the keys valid there.
    pub fn keys_hint(&self) -> &'static str {
        let msg = if self.confirm.is_some() {
            Msg::KeysConfirm
        } else {
            match &self.view {
                View::Menu if self.path.is_empty() => Msg::KeysMenuRoot,
                View::Menu => Msg::KeysMenu,
                View::Wizard(w) if w.text.is_some() => Msg::KeysText,
                View::Wizard(_) => Msg::KeysPicker,
                View::Output(o) if matches!(o.status, RunStatus::Running { .. }) => {
                    Msg::KeysRunning
                }
                View::Output(_) => Msg::KeysDone,
            }
        };
        self.lang.t(msg)
    }
}

/// Highest useful scroll: wrapped rows of the output minus the visible rows, for
/// a body of `(columns, rows)`. Unknown size (`0` columns): one row per line.
pub fn max_scroll(o: &Output, (columns, rows): (usize, usize)) -> usize {
    if columns == 0 {
        return o.lines.len();
    }
    text::wrapped_rows(o.lines.iter().map(|(_, l)| l.as_str()), columns).saturating_sub(rows)
}

fn interrupt_dialog(then_quit: bool, lang: Lang) -> ConfirmDialog {
    ConfirmDialog::new(
        Confirmation {
            question: lang.t(Msg::InterruptQ).to_string(),
            consequence: lang
                .t(if then_quit {
                    Msg::InterruptQuitC
                } else {
                    Msg::InterruptC
                })
                .to_string(),
        },
        Purpose::Interrupt { then_quit },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::i18n::Lang;
    use crate::console::state::{AgentState, InstanceInfo};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl_c() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
    }

    fn snap() -> Snapshot {
        Snapshot {
            installed: true,
            agents: crate::agents::AGENTS
                .iter()
                .map(|a| AgentState {
                    id: a.id,
                    display: a.display,
                    version: (a.id != "antigravity").then(|| "1.0".to_string()),
                    has_previous: a.id == "claude",
                    authenticated: a.id == "omlx",
                })
                .collect(),
            instances: vec![InstanceInfo {
                id: "1a2b3c4d".to_string(),
                agent: "claude".to_string(),
                project: "/tmp/p".to_string(),
            }],
            sessions: vec!["1a2b3c4d".to_string(), "00ff00ff".to_string()],
            tools: vec!["ripgrep".to_string()],
            omlx_url: Some("http://10.0.0.2:8000".to_string()),
            omlx_model: None,
            cwd: "/tmp/here".to_string(),
        }
    }

    fn press(app: &mut App, codes: &[KeyCode]) -> Effect {
        let mut last = Effect::None;
        for c in codes {
            last = app.handle_key(key(*c));
        }
        last
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
    }

    fn started(effect: Effect) -> Job {
        match effect {
            Effect::Start(job) => job,
            other => panic!("expected Start, got {other:?}"),
        }
    }

    #[test]
    fn arrows_wrap_and_enter_opens_a_submenu() {
        let mut app = App::new(snap(), Lang::Fr);
        press(&mut app, &[KeyCode::Up]);
        assert_eq!(app.cursor(), 7, "↑ on the first entry goes to the last");
        press(&mut app, &[KeyCode::Down]);
        assert_eq!(app.cursor(), 0);
        press(&mut app, &[KeyCode::Down, KeyCode::Enter]);
        assert_eq!(app.path, [1]);
        assert_eq!(app.path_title(), "zevbox › Mises à jour");
        press(&mut app, &[KeyCode::Left]);
        assert!(app.path.is_empty());
        assert_eq!(app.cursor(), 1, "back keeps the parent's selection");
        press(&mut app, &[KeyCode::Right]);
        assert_eq!(app.path, [1]);
        assert_eq!(press(&mut app, &[KeyCode::Esc]), Effect::None);
        assert_eq!(press(&mut app, &[KeyCode::Esc]), Effect::Quit);
    }

    #[test]
    fn focus_moves_to_the_right_inside_an_action() {
        let mut app = App::new(snap(), Lang::Fr);
        assert_eq!(app.focus(), Focus::Menu);
        press(&mut app, &[KeyCode::Enter]);
        assert_eq!(app.focus(), Focus::Menu, "a submenu is still the menu");
        assert!(app.keys_hint().starts_with("Menu :"));
        press(&mut app, &[KeyCode::Down, KeyCode::Enter]); // Arrêter un agent: picker
        assert_eq!(app.focus(), Focus::Action);
        assert!(app.keys_hint().starts_with("Liste à droite :"));
        press(&mut app, &[KeyCode::Esc]);
        assert_eq!(app.focus(), Focus::Menu);
        let _ = app.open_action(Action::Doctor);
        assert_eq!(app.focus(), Focus::Action);
        assert!(app.keys_hint().starts_with("Sortie à droite :"));
    }

    #[test]
    fn every_key_hint_fits_in_80_columns() {
        let mut app = App::new(snap(), Lang::Fr);
        let mut hints = vec![app.keys_hint()];
        press(&mut app, &[KeyCode::Enter]);
        hints.push(app.keys_hint());
        let _ = app.open_action(Action::StopOne);
        hints.push(app.keys_hint());
        let _ = app.open_action(Action::ToolsAdd);
        hints.push(app.keys_hint());
        let _ = app.open_action(Action::StopAll);
        hints.push(app.keys_hint());
        app.confirm = None;
        let _ = app.open_action(Action::Doctor);
        hints.push(app.keys_hint());
        app.finished(Outcome::Success);
        hints.push(app.keys_hint());
        for h in hints {
            assert!(text::width(h) <= 78, "{h}");
        }
        for msg in [
            Msg::KeysConfirm,
            Msg::KeysMenuRoot,
            Msg::KeysMenu,
            Msg::KeysText,
            Msg::KeysPicker,
            Msg::KeysRunning,
            Msg::KeysDone,
        ] {
            for lang in [Lang::En, Lang::Fr] {
                assert!(text::width(lang.t(msg)) <= 78, "{lang:?} {msg:?}");
            }
        }
    }

    #[test]
    fn page_keys_scroll_a_long_description_and_arrows_reset_it() {
        let mut app = App::new(snap(), Lang::Fr);
        app.output_body = (20, 5);
        press(&mut app, &[KeyCode::PageDown]);
        assert_eq!(app.desc_scroll, 5);
        press(
            &mut app,
            &[KeyCode::PageDown, KeyCode::PageDown, KeyCode::PageDown],
        );
        assert_eq!(app.desc_scroll, app.desc_max_scroll(), "stops at the end");
        assert!(app.desc_max_scroll() > 0);
        press(&mut app, &[KeyCode::PageUp]);
        assert_eq!(app.desc_scroll, app.desc_max_scroll() - 5);
        assert_eq!(app.cursor(), 0, "PgUp/PgDn never move the menu");
        press(&mut app, &[KeyCode::Down]);
        assert_eq!(app.desc_scroll, 0);
    }

    #[test]
    fn stderr_lines_are_marked() {
        let mut app = App::new(snap(), Lang::Fr);
        let _ = app.open_action(Action::Doctor);
        app.push_line(LineKind::Err, "zevbox: oops".to_string());
        match &app.view {
            View::Output(o) => assert_eq!(o.lines[0].1, "! zevbox: oops"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn english_outcomes_pickers_and_confirmations() {
        assert_eq!(Outcome::Success.label(Lang::En), "✓ done");
        assert_eq!(
            Outcome::Failed(Some(3)).label(Lang::En),
            "✗ failed (code 3)"
        );
        assert_eq!(
            Outcome::Interrupted.label(Lang::En),
            "✗ failed (interrupted)"
        );
        assert_eq!(
            Outcome::CannotStart.label(Lang::En),
            "✗ failed (could not start the command)"
        );
        let mut s = snap();
        s.instances.clear();
        let mut app = App::new(s, Lang::En);
        let p = app.picker_for(Step::Instance { allow_latest: true });
        assert_eq!(p.choices[0].label, "The most recent");
        let p = app.picker_for(Step::Instance {
            allow_latest: false,
        });
        assert_eq!(p.empty_message, "No agent running.");
        let _ = app.open_action(Action::StopAll);
        let c = app.confirm.as_ref().map(|d| d.confirmation.clone());
        assert_eq!(
            c.map(|c| (c.question, c.consequence)),
            Some((
                "Stop all agents?".to_string(),
                "The threads open in Zed will be cut.".to_string()
            ))
        );
        assert!(app.keys_hint().starts_with("Question:"));
    }

    #[test]
    fn q_and_ctrl_c_quit_from_the_menu() {
        let mut app = App::new(snap(), Lang::Fr);
        assert_eq!(press(&mut app, &[KeyCode::Char('q')]), Effect::Quit);
        assert_eq!(app.handle_key(ctrl_c()), Effect::Quit);
    }

    #[test]
    fn read_only_leaf_runs_at_once() {
        let mut app = App::new(snap(), Lang::Fr);
        let job = started(press(&mut app, &[KeyCode::Enter, KeyCode::Enter]));
        assert_eq!(job.invocations[0].args, ["status"]);
        assert!(!job.interactive);
        assert!(app.is_running());
    }

    #[test]
    fn stop_one_picks_an_instance_then_confirms_with_no_by_default() {
        let mut app = App::new(snap(), Lang::Fr);
        press(&mut app, &[KeyCode::Enter, KeyCode::Down, KeyCode::Enter]);
        assert!(matches!(&app.view, View::Wizard(w) if w.picker.is_some()));
        assert_eq!(press(&mut app, &[KeyCode::Enter]), Effect::None);
        let dialog = app.confirm.clone();
        assert!(dialog.as_ref().is_some_and(|d| !d.yes_selected));
        // Enter on the default button (Non) cancels.
        assert_eq!(press(&mut app, &[KeyCode::Enter]), Effect::None);
        assert!(app.confirm.is_none());
        assert_eq!(app.view, View::Menu);
        // Again, choosing Oui.
        press(&mut app, &[KeyCode::Enter, KeyCode::Enter, KeyCode::Tab]);
        let job = started(press(&mut app, &[KeyCode::Enter]));
        assert_eq!(job.invocations[0].args, ["stop", "1a2b3c4d"]);
    }

    #[test]
    fn confirm_dialog_keys() {
        let c = Confirmation {
            question: "?".to_string(),
            consequence: "!".to_string(),
        };
        let mut d = ConfirmDialog::new(c, Purpose::Interrupt { then_quit: false });
        assert!(!d.yes_selected, "Non by default");
        assert_eq!(d.key(KeyCode::Tab), None);
        assert!(d.yes_selected);
        assert_eq!(d.key(KeyCode::Left), None);
        assert!(!d.yes_selected);
        assert_eq!(d.key(KeyCode::Right), None);
        assert!(d.yes_selected);
        assert_eq!(d.key(KeyCode::Esc), Some(false), "Échap = Non");
        assert_eq!(d.key(KeyCode::Enter), Some(true));
        assert_eq!(d.key(KeyCode::Char('x')), None);
    }

    #[test]
    fn empty_picker_says_so_and_goes_back() {
        let mut s = snap();
        s.instances.clear();
        let mut app = App::new(s, Lang::Fr);
        press(&mut app, &[KeyCode::Enter, KeyCode::Down, KeyCode::Enter]);
        match &app.view {
            View::Wizard(w) => {
                let p = w.picker.as_ref().unwrap();
                assert!(p.choices.is_empty());
                assert_eq!(p.empty_message, "Aucun agent en cours.");
            }
            other => panic!("{other:?}"),
        }
        press(&mut app, &[KeyCode::Enter]);
        assert_eq!(app.view, View::Menu);
    }

    #[test]
    fn logs_offers_the_latest_first() {
        let mut app = App::new(snap(), Lang::Fr);
        press(&mut app, &[KeyCode::Enter, KeyCode::Up, KeyCode::Enter]);
        let job = started(press(&mut app, &[KeyCode::Enter]));
        assert_eq!(job.invocations[0].args, ["logs"]);
        let mut app = App::new(snap(), Lang::Fr);
        let _ = app.open_action(Action::Logs);
        press(&mut app, &[KeyCode::Down, KeyCode::Down]);
        let job = started(press(&mut app, &[KeyCode::Enter]));
        assert_eq!(job.invocations[0].args, ["logs", "00ff00ff"]);
    }

    #[test]
    fn rollback_lists_only_agents_with_a_previous_release() {
        let app = App::new(snap(), Lang::Fr);
        let p = app.picker_for(Step::Agent(AgentSet::WithPrevious));
        let ids: Vec<_> = p.choices.iter().map(|c| c.value.as_str()).collect();
        assert_eq!(ids, ["claude"]);
        let p = app.picker_for(Step::Agent(AgentSet::Installed));
        let ids: Vec<_> = p.choices.iter().map(|c| c.value.as_str()).collect();
        assert_eq!(ids, ["claude", "omlx"]);
    }

    #[test]
    fn omlx_url_is_prefilled_key_masked_and_never_in_argv() {
        let mut app = App::new(snap(), Lang::Fr);
        // Connexion des agents › Connecter oMLX.
        press(
            &mut app,
            &[
                KeyCode::Down,
                KeyCode::Down,
                KeyCode::Enter,
                KeyCode::Up,
                KeyCode::Enter,
            ],
        );
        match &app.view {
            View::Wizard(w) => {
                assert_eq!(w.text.as_ref().unwrap().value(), "http://10.0.0.2:8000")
            }
            other => panic!("{other:?}"),
        }
        press(&mut app, &[KeyCode::Enter]);
        type_text(&mut app, "sk-q-secret");
        match &app.view {
            View::Wizard(w) => {
                let t = w.text.as_ref().unwrap();
                assert_eq!(t.shown(), "•".repeat(11), "q is typed, not quit");
                assert!(!t.shown().contains("secret"));
            }
            other => panic!("{other:?}"),
        }
        press(&mut app, &[KeyCode::Enter, KeyCode::Enter]); // key, then empty model
                                                            // oMLX is already connected: confirmation first.
        assert!(app.confirm.is_some());
        let job = started(press(&mut app, &[KeyCode::Tab, KeyCode::Enter]));
        let inv = &job.invocations[0];
        assert_eq!(inv.args, ["auth", "omlx", "--url", "http://10.0.0.2:8000"]);
        assert!(!inv.args.iter().any(|a| a.contains("secret")));
        assert_eq!(
            inv.stdin.as_ref().map(Secret::expose),
            Some("sk-q-secret\n")
        );
    }

    #[test]
    fn omlx_key_is_required_when_not_connected() {
        let mut s = snap();
        for a in &mut s.agents {
            a.authenticated = false;
        }
        let mut app = App::new(s, Lang::Fr);
        let _ = app.open_action(Action::AuthOmlx);
        press(&mut app, &[KeyCode::Enter, KeyCode::Enter]);
        match &app.view {
            View::Wizard(w) => assert_eq!(w.step, 1, "empty key refused"),
            other => panic!("{other:?}"),
        }
        press(&mut app, &[KeyCode::Esc]);
        match &app.view {
            View::Wizard(w) => assert_eq!(w.step, 0, "Échap goes back one step"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn shell_project_is_prefilled_with_the_current_directory() {
        let mut app = App::new(snap(), Lang::Fr);
        let _ = app.open_action(Action::Shell);
        let job = started(press(&mut app, &[KeyCode::Enter, KeyCode::Enter]));
        assert!(job.interactive);
        assert_eq!(
            job.invocations[0].args,
            ["shell", "claude", "--project", "/tmp/here"]
        );
        assert_eq!(
            app.view,
            View::Menu,
            "interactive runs leave the menu shown"
        );
    }

    #[test]
    fn tools_add_splits_names_and_confirms() {
        let mut app = App::new(snap(), Lang::Fr);
        let _ = app.open_action(Action::ToolsAdd);
        type_text(&mut app, " git  jq ");
        press(&mut app, &[KeyCode::Enter]);
        let question = app
            .confirm
            .as_ref()
            .map(|d| d.confirmation.question.clone());
        assert_eq!(question.as_deref(), Some("Ajouter git jq ?"));
        let job = started(press(&mut app, &[KeyCode::Right, KeyCode::Enter]));
        assert_eq!(job.invocations[0].args, ["tools", "add", "--", "git", "jq"]);
    }

    #[test]
    fn escape_during_a_run_asks_before_interrupting() {
        let mut app = App::new(snap(), Lang::Fr);
        let _ = app.open_action(Action::Doctor);
        assert!(app.is_running());
        press(&mut app, &[KeyCode::Esc]);
        assert!(app.confirm.is_some());
        assert_eq!(
            press(&mut app, &[KeyCode::Esc]),
            Effect::None,
            "Échap = Non"
        );
        assert!(app.is_running());
        press(&mut app, &[KeyCode::Esc, KeyCode::Tab]);
        assert_eq!(press(&mut app, &[KeyCode::Enter]), Effect::Interrupt);
        assert!(!app.quit_after_run);
        app.finished(Outcome::Interrupted);
        assert!(!app.is_running());
        assert_eq!(press(&mut app, &[KeyCode::Esc]), Effect::None);
        assert_eq!(app.view, View::Menu);
    }

    #[test]
    fn quit_during_a_run_interrupts_first() {
        let mut app = App::new(snap(), Lang::Fr);
        let _ = app.open_action(Action::Doctor);
        assert_eq!(app.handle_key(ctrl_c()), Effect::None);
        press(&mut app, &[KeyCode::Tab]);
        assert_eq!(press(&mut app, &[KeyCode::Enter]), Effect::Interrupt);
        assert!(app.quit_after_run);
    }

    #[test]
    fn scrolling_counts_wrapped_rows_and_stops_at_the_top() {
        let mut app = App::new(snap(), Lang::Fr);
        app.output_body = (10, 4);
        let _ = app.open_action(Action::Logs);
        let _ = app.handle_key(key(KeyCode::Enter));
        // 3 lines of 25 columns = 9 rows at 10 columns; 4 visible → 5 above.
        for _ in 0..3 {
            app.push_line(LineKind::Out, "x".repeat(25));
        }
        let scroll = |app: &App| match &app.view {
            View::Output(o) => o.scroll,
            other => panic!("{other:?}"),
        };
        press(&mut app, &[KeyCode::PageUp]);
        assert_eq!(scroll(&app), 4, "a page = the visible rows");
        press(&mut app, &[KeyCode::PageUp]);
        assert_eq!(
            scroll(&app),
            5,
            "every wrapped row is reachable, no further"
        );
        press(&mut app, &[KeyCode::Up, KeyCode::Up]);
        assert_eq!(scroll(&app), 5);
        press(&mut app, &[KeyCode::Down]);
        assert_eq!(scroll(&app), 4, "no dead zone: ↓ moves at once");
        press(&mut app, &[KeyCode::PageDown]);
        assert_eq!(scroll(&app), 0);
        // Short output: nothing to scroll.
        let mut app = App::new(snap(), Lang::Fr);
        app.output_body = (40, 10);
        let _ = app.open_action(Action::Status);
        app.push_line(LineKind::Out, "one".to_string());
        press(&mut app, &[KeyCode::PageUp]);
        assert_eq!(scroll(&app), 0);
    }

    #[test]
    fn text_field_refuses_a_leading_dash_and_says_why() {
        let mut app = App::new(snap(), Lang::Fr);
        let _ = app.open_action(Action::ToolsAdd);
        type_text(&mut app, "git -y");
        press(&mut app, &[KeyCode::Enter]);
        assert!(app.confirm.is_none());
        match &app.view {
            View::Wizard(w) => assert!(w.text.as_ref().unwrap().error.is_some()),
            other => panic!("{other:?}"),
        }
        press(&mut app, &[KeyCode::Backspace]);
        match &app.view {
            View::Wizard(w) => assert!(w.text.as_ref().unwrap().error.is_none()),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn omlx_model_is_prefilled_with_the_current_default() {
        let mut s = snap();
        s.omlx_model = Some("Qwen3-8B".to_string());
        let mut app = App::new(s, Lang::Fr);
        let _ = app.open_action(Action::AuthOmlx);
        press(&mut app, &[KeyCode::Enter, KeyCode::Enter]); // URL, key kept
        match &app.view {
            View::Wizard(w) => assert_eq!(w.text.as_ref().unwrap().value(), "Qwen3-8B"),
            other => panic!("{other:?}"),
        }
        press(&mut app, &[KeyCode::Enter, KeyCode::Tab]);
        let job = started(press(&mut app, &[KeyCode::Enter]));
        assert!(job.invocations[0]
            .args
            .ends_with(&["--model".to_string(), "Qwen3-8B".to_string()]));
    }

    #[test]
    fn debug_of_a_masked_field_hides_the_key() {
        let mut app = App::new(snap(), Lang::Fr);
        let _ = app.open_action(Action::AuthOmlx);
        press(&mut app, &[KeyCode::Enter]);
        type_text(&mut app, "sk-hidden");
        let dump = format!("{:?}", app.view);
        assert!(!dump.contains("sk-hidden"), "{dump}");
        assert!(dump.contains("•••••••••"));
        press(&mut app, &[KeyCode::Enter]);
        assert!(!format!("{:?}", app.view).contains("sk-hidden"));
    }

    #[test]
    fn output_scrolls_and_keeps_a_bounded_history() {
        let mut app = App::new(snap(), Lang::Fr);
        let _ = app.open_action(Action::Status);
        for i in 0..(MAX_OUTPUT_LINES + 3) {
            app.push_line(LineKind::Out, i.to_string());
        }
        press(&mut app, &[KeyCode::PageUp, KeyCode::Up, KeyCode::Down]);
        match &app.view {
            View::Output(o) => {
                assert_eq!(o.lines.len(), MAX_OUTPUT_LINES);
                assert_eq!(o.lines[0].1, "3");
                assert_eq!(o.scroll, PAGE);
            }
            other => panic!("{other:?}"),
        }
        app.finished(Outcome::Success);
        assert_eq!(press(&mut app, &[KeyCode::Char('q')]), Effect::Quit);
    }
}

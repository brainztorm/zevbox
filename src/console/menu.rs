//! Pure menu model of `zevbox console` (docs/SPEC_console.md §5, §6): the tree of
//! menus, what each leaf asks for, the `zevbox …` arguments it builds and whether
//! it needs « Êtes-vous sûr ? ». No I/O here: everything is unit-tested.
//!
//! The console never reimplements a command (§2): a leaf only turns the chosen
//! parameters into the argument vector of the current binary.

use super::i18n::{Lang, Msg};
use super::state::Snapshot;

/// One entry of a menu level: a submenu (marked `›`) or a leaf (an action).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Menu(Menu),
    Leaf(Action),
}

impl Entry {
    pub fn label(&self, lang: Lang) -> &'static str {
        match self {
            Entry::Menu(m) => lang.t(m.label),
            Entry::Leaf(a) => a.label(lang),
        }
    }

    pub fn description(&self, lang: Lang) -> &'static str {
        match self {
            Entry::Menu(m) => lang.t(m.description),
            Entry::Leaf(a) => a.description(lang),
        }
    }
}

/// A submenu; its texts are keys of the string table (`i18n.rs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Menu {
    pub label: Msg,
    pub description: Msg,
    pub children: Vec<Entry>,
}

/// Every leaf of §5. `launch`, `init`, `logs --path` and `zed install --settings`
/// are deliberately absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Status,
    StopOne,
    StopAll,
    Logs,
    UpdateCheck,
    UpdateAll,
    UpdateAgent,
    Rollback,
    AuthStatus,
    AuthClaude,
    AuthAntigravity,
    AuthOmlx,
    ToolsList,
    ToolsAdd,
    ToolsRemove,
    Doctor,
    Clean,
    CleanImages,
    ZedDryRun,
    ZedInstall,
    ZedUninstall,
    Shell,
    Setup,
}

/// Which agents a picker offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSet {
    /// Every agent of `agents::AGENTS`.
    All,
    /// Agents with an active release.
    Installed,
    /// Agents whose active entry has a `previous` release.
    WithPrevious,
}

/// Free-text fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextField {
    /// One or more apt package names separated by spaces.
    Packages,
    /// oMLX server URL, prefilled with the current one.
    Url,
    /// oMLX API key: masked, sent on stdin only.
    Key,
    /// oMLX default model (optional).
    Model,
    /// Project of `zevbox shell`, prefilled with the current directory.
    Project,
}

impl TextField {
    pub fn prompt(self) -> Msg {
        match self {
            TextField::Packages => Msg::PromptPackages,
            TextField::Url => Msg::PromptUrl,
            TextField::Key => Msg::PromptKey,
            TextField::Model => Msg::PromptModel,
            TextField::Project => Msg::PromptProject,
        }
    }

    pub fn masked(self) -> bool {
        self == TextField::Key
    }

    /// Refuses what would reach `zevbox` as an option instead of a value. The key
    /// goes on stdin, never in argv, so it is free.
    pub fn check(self, value: &str) -> Result<(), Msg> {
        let dash = match self {
            TextField::Key => false,
            TextField::Packages => value.split_whitespace().any(|w| w.starts_with('-')),
            TextField::Url | TextField::Model | TextField::Project => value.trim().starts_with('-'),
        };
        if dash {
            Err(Msg::DashRefused)
        } else {
            Ok(())
        }
    }
}

/// One parameter a leaf asks for, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Pick an instance; `allow_latest` adds « la plus récente » first.
    Instance {
        allow_latest: bool,
    },
    Agent(AgentSet),
    /// Pick one tool of `tools.json`.
    Tool,
    Text(TextField),
}

/// Instance chosen for `stop` / `logs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceChoice {
    Latest,
    Id(String),
}

/// A secret typed in the console (the oMLX key). `Debug` never shows it, so a
/// stray `{:?}` cannot leak it into a log (I9).
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: String) -> Self {
        Self(value)
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Best-effort wipe of a secret string before its memory is freed (no extra
/// crate, no `unsafe`: `black_box` keeps the writes from being optimised away).
pub fn wipe(text: &mut String) {
    let mut bytes = std::mem::take(text).into_bytes();
    bytes.iter_mut().for_each(|b| *b = 0);
    std::hint::black_box(&bytes);
}

impl Drop for Secret {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***)")
    }
}

/// Parameters chosen so far for one leaf. `None` = not chosen yet.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Params {
    pub instance: Option<InstanceChoice>,
    pub agent: Option<String>,
    pub tool: Option<String>,
    pub packages: Vec<String>,
    pub url: Option<String>,
    /// Empty = keep the current key (`auth omlx` reads an empty line that way).
    pub key: Option<Secret>,
    pub model: Option<String>,
    pub project: Option<String>,
}

/// One run of the current binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    /// Arguments after the binary, one by one (no shell, I7).
    pub args: Vec<String>,
    /// Written to the child's stdin, then stdin is closed. Never in argv/env.
    pub stdin: Option<Secret>,
    /// Handed the real terminal (§7).
    pub interactive: bool,
}

impl Invocation {
    fn plain(args: &[&str]) -> Self {
        Self {
            args: args.iter().map(|s| s.to_string()).collect(),
            stdin: None,
            interactive: false,
        }
    }

    fn interactive(args: Vec<String>) -> Self {
        Self {
            args,
            stdin: None,
            interactive: true,
        }
    }

    /// `zevbox <args>` as shown to the user (never contains the secret).
    pub fn display(&self) -> String {
        std::iter::once("zevbox".to_string())
            .chain(self.args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Kind of a line of the description panel; the drawing picks the colour
/// (one meaning per colour, docs/SPEC_console.md §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Body,
    /// Secondary text: duration, network, the `Commande :` line.
    Dim,
    /// OK: « connecté ».
    Good,
    /// Attention: « non connecté ».
    Bad,
}

/// One logical line of the description panel (wrapped when drawn).
pub type PanelLine = (Tone, String);

fn line(tone: Tone, text: impl Into<String>) -> PanelLine {
    (tone, text.into())
}

/// Right panel of a submenu: its explanation and what it contains.
pub fn menu_panel(menu: &Menu, lang: Lang) -> Vec<PanelLine> {
    let mut lines = vec![
        line(Tone::Body, lang.t(menu.description)),
        line(Tone::Body, ""),
        line(Tone::Dim, lang.t(Msg::Contains)),
    ];
    lines.extend(
        menu.children
            .iter()
            .map(|c| line(Tone::Body, format!("  · {}", c.label(lang)))),
    );
    lines
}

/// When a leaf asks « Êtes-vous sûr ? » (§6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmRule {
    Never,
    Always,
    /// Only when this agent is already connected (redoing a login).
    IfAuthenticated(&'static str),
}

/// Text of a confirmation box: the question and its concrete consequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirmation {
    pub question: String,
    pub consequence: String,
}

impl Action {
    pub fn label(self, lang: Lang) -> &'static str {
        lang.t(self.label_msg())
    }

    pub fn label_msg(self) -> Msg {
        match self {
            Action::Status => Msg::StatusLabel,
            Action::StopOne => Msg::StopOneLabel,
            Action::StopAll => Msg::StopAllLabel,
            Action::Logs => Msg::LogsLabel,
            Action::UpdateCheck => Msg::UpdateCheckLabel,
            Action::UpdateAll => Msg::UpdateAllLabel,
            Action::UpdateAgent => Msg::UpdateAgentLabel,
            Action::Rollback => Msg::RollbackLabel,
            Action::AuthStatus => Msg::AuthStatusLabel,
            Action::AuthClaude => Msg::AuthClaudeLabel,
            Action::AuthAntigravity => Msg::AuthAntigravityLabel,
            Action::AuthOmlx => Msg::AuthOmlxLabel,
            Action::ToolsList => Msg::ToolsListLabel,
            Action::ToolsAdd => Msg::ToolsAddLabel,
            Action::ToolsRemove => Msg::ToolsRemoveLabel,
            Action::Doctor => Msg::DoctorLabel,
            Action::Clean => Msg::CleanLabel,
            Action::CleanImages => Msg::CleanImagesLabel,
            Action::ZedDryRun => Msg::ZedDryRunLabel,
            Action::ZedInstall => Msg::ZedInstallLabel,
            Action::ZedUninstall => Msg::ZedUninstallLabel,
            Action::Shell => Msg::ShellLabel,
            Action::Setup => Msg::SetupLabel,
        }
    }

    /// Plain-words explanation for a non-expert (feedback 👤 2026-10-30): what it
    /// does, when to use it, what it changes and what it leaves alone. Facts
    /// from the commands themselves and docs/USER_GUIDE.md.
    pub fn description(self, lang: Lang) -> &'static str {
        lang.t(self.description_msg())
    }

    pub fn description_msg(self) -> Msg {
        match self {
            Action::Status => Msg::StatusDesc,
            Action::StopOne => Msg::StopOneDesc,
            Action::StopAll => Msg::StopAllDesc,
            Action::Logs => Msg::LogsDesc,
            Action::UpdateCheck => Msg::UpdateCheckDesc,
            Action::UpdateAll => Msg::UpdateAllDesc,
            Action::UpdateAgent => Msg::UpdateAgentDesc,
            Action::Rollback => Msg::RollbackDesc,
            Action::AuthStatus => Msg::AuthStatusDesc,
            Action::AuthClaude => Msg::AuthClaudeDesc,
            Action::AuthAntigravity => Msg::AuthAntigravityDesc,
            Action::AuthOmlx => Msg::AuthOmlxDesc,
            Action::ToolsList => Msg::ToolsListDesc,
            Action::ToolsAdd => Msg::ToolsAddDesc,
            Action::ToolsRemove => Msg::ToolsRemoveDesc,
            Action::Doctor => Msg::DoctorDesc,
            Action::Clean => Msg::CleanDesc,
            Action::CleanImages => Msg::CleanImagesDesc,
            Action::ZedDryRun => Msg::ZedDryRunDesc,
            Action::ZedInstall => Msg::ZedInstallDesc,
            Action::ZedUninstall => Msg::ZedUninstallDesc,
            Action::Shell => Msg::ShellDesc,
            Action::Setup => Msg::SetupDesc,
        }
    }

    /// « Duration … · network … » line, when useful.
    pub fn cost(self, lang: Lang) -> Option<&'static str> {
        let msg = match self {
            Action::StopOne | Action::StopAll => Msg::CostStop,
            Action::UpdateCheck => Msg::CostCheck,
            Action::UpdateAll => Msg::CostUpdateAll,
            Action::UpdateAgent => Msg::CostUpdateAgent,
            Action::AuthClaude | Action::AuthAntigravity => Msg::CostSignIn,
            Action::AuthOmlx => Msg::CostOmlx,
            Action::ToolsAdd | Action::ToolsRemove => Msg::CostTools,
            Action::Doctor | Action::Clean | Action::CleanImages => Msg::CostSeconds,
            Action::Shell => Msg::CostShell,
            Action::Setup => Msg::CostSetup,
            _ => return None,
        };
        Some(lang.t(msg))
    }

    /// Parameters asked for, in order.
    pub fn steps(self) -> &'static [Step] {
        match self {
            Action::StopOne => &[Step::Instance {
                allow_latest: false,
            }],
            Action::Logs => &[Step::Instance { allow_latest: true }],
            Action::UpdateAgent => &[Step::Agent(AgentSet::All)],
            Action::Rollback => &[Step::Agent(AgentSet::WithPrevious)],
            Action::AuthOmlx => &[
                Step::Text(TextField::Url),
                Step::Text(TextField::Key),
                Step::Text(TextField::Model),
            ],
            Action::ToolsAdd => &[Step::Text(TextField::Packages)],
            Action::ToolsRemove => &[Step::Tool],
            Action::Shell => &[
                Step::Agent(AgentSet::Installed),
                Step::Text(TextField::Project),
            ],
            _ => &[],
        }
    }

    /// Handed the real terminal instead of the output panel (§7).
    pub fn is_interactive(self) -> bool {
        matches!(
            self,
            Action::Shell | Action::AuthClaude | Action::AuthAntigravity | Action::Setup
        )
    }

    /// §6: what asks « Êtes-vous sûr ? ». Read-only leaves, `clean`, `zed install`
    /// and `setup` never do.
    /// Right panel of a leaf: explanation, connection state (sign-in leaves),
    /// duration · network, then `Commande : zevbox …` with the choices so far.
    pub fn panel(self, params: &Params, snap: &Snapshot, lang: Lang) -> Vec<PanelLine> {
        let mut lines = vec![line(Tone::Body, self.description(lang))];
        if let Some(agent) = self.auth_agent() {
            lines.push(line(Tone::Body, ""));
            lines.push(if snap.is_authenticated(agent) {
                line(Tone::Good, lang.t(Msg::Connected))
            } else {
                line(Tone::Bad, lang.t(Msg::NotConnected))
            });
        }
        if let Some(cost) = self.cost(lang) {
            lines.push(line(Tone::Body, ""));
            lines.push(line(Tone::Dim, cost));
        }
        if let Some(note) = self.cost_note(snap, lang) {
            lines.push(line(Tone::Body, note));
        }
        lines.push(line(Tone::Body, ""));
        lines.push(line(
            Tone::Dim,
            lang.f(Msg::CommandLine, &[&self.preview(params, lang)]),
        ));
        lines
    }

    /// The agent a « Se connecter » leaf logs in.
    pub fn auth_agent(self) -> Option<&'static str> {
        match self {
            Action::AuthClaude => Some("claude"),
            Action::AuthAntigravity => Some("antigravity"),
            Action::AuthOmlx => Some("omlx"),
            _ => None,
        }
    }

    /// Extra line under [`Action::cost`] that depends on the installation: the
    /// first login of an agent without a release builds its image first
    /// (`auth` quick start, DECISIONS R2).
    pub fn cost_note(self, snap: &Snapshot, lang: Lang) -> Option<&'static str> {
        let agent = self.auth_agent()?;
        let installed = snap
            .agents
            .iter()
            .any(|a| a.id == agent && a.version.is_some());
        (!installed).then_some(lang.t(Msg::FirstSignInBuilds))
    }

    pub fn confirm_rule(self) -> ConfirmRule {
        match self {
            Action::StopOne
            | Action::StopAll
            | Action::UpdateAll
            | Action::UpdateAgent
            | Action::Rollback
            | Action::ToolsAdd
            | Action::ToolsRemove
            | Action::CleanImages
            | Action::ZedUninstall => ConfirmRule::Always,
            _ => match self.auth_agent() {
                Some(agent) => ConfirmRule::IfAuthenticated(agent),
                None => ConfirmRule::Never,
            },
        }
    }

    /// The box to show before running, or `None` to run at once.
    pub fn confirmation(
        self,
        params: &Params,
        snap: &Snapshot,
        lang: Lang,
    ) -> Option<Confirmation> {
        let display = |id: &str| snap.display_of(id);
        let agent = params.agent.as_deref().unwrap_or("");
        let pair = |q: Msg, c: Msg| (lang.t(q).to_string(), lang.t(c).to_string());
        let (question, consequence) = match self.confirm_rule() {
            ConfirmRule::Never => return None,
            ConfirmRule::IfAuthenticated(id) if !snap.is_authenticated(id) => return None,
            ConfirmRule::IfAuthenticated(id) => (
                lang.f(Msg::ReauthQ, &[&display(id)]),
                lang.t(Msg::ReauthC).to_string(),
            ),
            ConfirmRule::Always => match self {
                Action::StopOne => {
                    let id = match &params.instance {
                        Some(InstanceChoice::Id(id)) => id.as_str(),
                        _ => "",
                    };
                    (
                        lang.f(Msg::StopOneQ, &[id]),
                        lang.t(Msg::StopOneC).to_string(),
                    )
                }
                Action::StopAll => pair(Msg::StopAllQ, Msg::StopAllC),
                Action::UpdateAll => pair(Msg::UpdateAllQ, Msg::UpdateAllC),
                Action::UpdateAgent => (
                    lang.f(Msg::UpdateAgentQ, &[&display(agent)]),
                    lang.t(Msg::UpdateAgentC).to_string(),
                ),
                Action::Rollback => (
                    lang.f(Msg::RollbackQ, &[&display(agent)]),
                    lang.t(Msg::RollbackC).to_string(),
                ),
                Action::ToolsAdd => (
                    lang.f(Msg::ToolsAddQ, &[&params.packages.join(" ")]),
                    lang.t(Msg::ToolsC).to_string(),
                ),
                Action::ToolsRemove => (
                    lang.f(Msg::ToolsRemoveQ, &[params.tool.as_deref().unwrap_or("")]),
                    lang.t(Msg::ToolsC).to_string(),
                ),
                Action::CleanImages => pair(Msg::CleanImagesQ, Msg::CleanImagesC),
                Action::ZedUninstall => pair(Msg::ZedUninstallQ, Msg::ZedUninstallC),
                _ => return None,
            },
        };
        Some(Confirmation {
            question,
            consequence,
        })
    }

    /// The runs of the current binary for this leaf, in order. Only meaningful once
    /// every step has a value (see [`Action::preview`] for the partial form).
    pub fn invocations(self, p: &Params) -> Vec<Invocation> {
        let agent = p.agent.clone().unwrap_or_default();
        match self {
            Action::Status => vec![Invocation::plain(&["status"])],
            Action::StopOne => {
                let id = match &p.instance {
                    Some(InstanceChoice::Id(id)) => id.clone(),
                    _ => String::new(),
                };
                vec![Invocation::plain(&["stop", &id])]
            }
            Action::StopAll => vec![Invocation::plain(&["stop"])],
            Action::Logs => match &p.instance {
                Some(InstanceChoice::Id(id)) => vec![Invocation::plain(&["logs", id])],
                _ => vec![Invocation::plain(&["logs"])],
            },
            Action::UpdateCheck => vec![Invocation::plain(&["update", "--check"])],
            Action::UpdateAll => vec![Invocation::plain(&["update"])],
            Action::UpdateAgent => vec![Invocation::plain(&["update", "--agent", &agent])],
            Action::Rollback => vec![Invocation::plain(&["rollback", "--agent", &agent])],
            Action::AuthStatus => crate::agents::AGENTS
                .iter()
                .map(|a| Invocation::plain(&["auth", a.id, "--status"]))
                .collect(),
            Action::AuthClaude => vec![Invocation::interactive(vec![
                "auth".to_string(),
                "claude".to_string(),
            ])],
            Action::AuthAntigravity => vec![Invocation::interactive(vec![
                "auth".to_string(),
                "antigravity".to_string(),
            ])],
            Action::AuthOmlx => {
                let mut args = vec![
                    "auth".to_string(),
                    "omlx".to_string(),
                    "--url".to_string(),
                    p.url.clone().unwrap_or_default(),
                ];
                if let Some(model) = p.model.as_ref().filter(|m| !m.is_empty()) {
                    args.push("--model".to_string());
                    args.push(model.clone());
                }
                // The key goes on stdin, one line; an empty line keeps the current key
                // (cli/auth_omlx.rs `read_secret` on a pipe).
                let key = p.key.as_ref().map_or("", Secret::expose);
                vec![Invocation {
                    args,
                    stdin: Some(Secret::new(format!("{key}\n"))),
                    interactive: false,
                }]
            }
            Action::ToolsList => vec![Invocation::plain(&["tools", "list"])],
            Action::ToolsAdd => {
                // `--`: whatever the names, clap reads them as names, never options.
                let mut args = vec!["tools".to_string(), "add".to_string(), "--".to_string()];
                args.extend(p.packages.iter().cloned());
                vec![Invocation {
                    args,
                    stdin: None,
                    interactive: false,
                }]
            }
            Action::ToolsRemove => vec![Invocation::plain(&[
                "tools",
                "remove",
                "--",
                p.tool.as_deref().unwrap_or(""),
            ])],
            Action::Doctor => vec![Invocation::plain(&["doctor"])],
            Action::Clean => vec![Invocation::plain(&["clean"])],
            Action::CleanImages => vec![Invocation::plain(&["clean", "--images"])],
            Action::ZedDryRun => vec![Invocation::plain(&["zed", "install", "--dry-run"])],
            Action::ZedInstall => vec![Invocation::plain(&["zed", "install"])],
            Action::ZedUninstall => vec![Invocation::plain(&["zed", "uninstall"])],
            Action::Shell => vec![Invocation::interactive(vec![
                "shell".to_string(),
                agent,
                "--project".to_string(),
                p.project.clone().unwrap_or_default(),
            ])],
            Action::Setup => vec![Invocation::interactive(vec!["setup".to_string()])],
        }
    }

    /// « zevbox … » line of the description panel, with the parameters chosen so
    /// far and placeholders for the others. Never shows the key.
    pub fn preview(self, p: &Params, lang: Lang) -> String {
        let or = |v: &Option<String>, placeholder: &str| {
            v.clone().unwrap_or_else(|| placeholder.to_string())
        };
        match self {
            Action::StopOne if p.instance.is_none() => "zevbox stop <id>".to_string(),
            Action::Logs if p.instance.is_none() => "zevbox logs [<id>]".to_string(),
            Action::UpdateAgent if p.agent.is_none() => "zevbox update --agent <agent>".to_string(),
            Action::Rollback if p.agent.is_none() => "zevbox rollback --agent <agent>".to_string(),
            Action::AuthStatus => {
                format!("zevbox auth <agent> --status {}", lang.t(Msg::ForEachAgent))
            }
            Action::AuthOmlx => {
                let model = match &p.model {
                    Some(m) if !m.is_empty() => format!(" --model {m}"),
                    Some(_) => String::new(),
                    None => format!(" [--model {}]", lang.t(Msg::PhModel)),
                };
                format!(
                    "zevbox auth omlx --url {}{model} {}",
                    or(&p.url, lang.t(Msg::PhAddress)),
                    lang.t(Msg::KeyOnStdin)
                )
            }
            Action::ToolsAdd if p.packages.is_empty() => {
                format!("zevbox tools add {}…", lang.t(Msg::PhPackage))
            }
            Action::ToolsRemove if p.tool.is_none() => {
                format!("zevbox tools remove {}", lang.t(Msg::PhPackage))
            }
            Action::Shell => format!(
                "zevbox shell {} --project {}",
                or(&p.agent, "<agent>"),
                or(&p.project, lang.t(Msg::PhProject))
            ),
            _ => self
                .invocations(p)
                .first()
                .map(Invocation::display)
                .unwrap_or_default(),
        }
    }
}

/// The tree of §5 (top level = the children of « zevbox »).
pub fn root() -> Menu {
    let leaf = Entry::Leaf;
    let menu = |label, description, children| {
        Entry::Menu(Menu {
            label,
            description,
            children,
        })
    };
    Menu {
        label: Msg::Root,
        description: Msg::Root,
        children: vec![
            menu(
                Msg::MenuAgents,
                Msg::MenuAgentsDesc,
                vec![
                    leaf(Action::Status),
                    leaf(Action::StopOne),
                    leaf(Action::StopAll),
                    leaf(Action::Logs),
                ],
            ),
            menu(
                Msg::MenuUpdates,
                Msg::MenuUpdatesDesc,
                vec![
                    leaf(Action::UpdateCheck),
                    leaf(Action::UpdateAll),
                    leaf(Action::UpdateAgent),
                    leaf(Action::Rollback),
                ],
            ),
            menu(
                Msg::MenuAuth,
                Msg::MenuAuthDesc,
                vec![
                    leaf(Action::AuthStatus),
                    leaf(Action::AuthClaude),
                    leaf(Action::AuthAntigravity),
                    leaf(Action::AuthOmlx),
                ],
            ),
            menu(
                Msg::MenuTools,
                Msg::MenuToolsDesc,
                vec![
                    leaf(Action::ToolsList),
                    leaf(Action::ToolsAdd),
                    leaf(Action::ToolsRemove),
                ],
            ),
            menu(
                Msg::MenuCare,
                Msg::MenuCareDesc,
                vec![
                    leaf(Action::Doctor),
                    leaf(Action::Clean),
                    leaf(Action::CleanImages),
                ],
            ),
            menu(
                Msg::MenuZed,
                Msg::MenuZedDesc,
                vec![
                    leaf(Action::ZedDryRun),
                    leaf(Action::ZedInstall),
                    leaf(Action::ZedUninstall),
                ],
            ),
            leaf(Action::Shell),
            leaf(Action::Setup),
        ],
    }
}

/// The menu reached by opening the submenus at `path` (indices), if valid.
pub fn menu_at<'a>(root: &'a Menu, path: &[usize]) -> Option<&'a Menu> {
    let mut current = root;
    for &i in path {
        match current.children.get(i) {
            Some(Entry::Menu(m)) => current = m,
            _ => return None,
        }
    }
    Some(current)
}

/// Every leaf of the tree, depth first.
pub fn all_actions(menu: &Menu) -> Vec<Action> {
    let mut out = Vec::new();
    for entry in &menu.children {
        match entry {
            Entry::Menu(m) => out.extend(all_actions(m)),
            Entry::Leaf(a) => out.push(*a),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::state::{AgentState, Snapshot};

    fn snap(authenticated: &[&str]) -> Snapshot {
        Snapshot {
            installed: true,
            agents: crate::agents::AGENTS
                .iter()
                .map(|a| AgentState {
                    id: a.id,
                    display: a.display,
                    version: Some("1.0".to_string()),
                    has_previous: false,
                    authenticated: authenticated.contains(&a.id),
                })
                .collect(),
            ..Snapshot::default()
        }
    }

    fn args(inv: &[Invocation]) -> Vec<Vec<&str>> {
        inv.iter()
            .map(|i| i.args.iter().map(String::as_str).collect())
            .collect()
    }

    #[test]
    fn tree_has_the_menus_of_section_5_in_order() {
        let root = root();
        let labels: Vec<_> = root.children.iter().map(|e| e.label(Lang::Fr)).collect();
        assert_eq!(
            labels,
            [
                "Agents en cours",
                "Mises à jour",
                "Connexion des agents",
                "Outils Linux",
                "Diagnostic et ménage",
                "Zed",
                "Shell dans une VM",
                "Première installation",
            ]
        );
        let updates = menu_at(&root, &[1]).map(|m| m.children.len());
        assert_eq!(updates, Some(4));
        assert!(menu_at(&root, &[6]).is_none(), "a leaf is not a menu");
        assert!(menu_at(&root, &[42]).is_none());
        assert_eq!(all_actions(&root).len(), 23);
    }

    #[test]
    fn every_leaf_builds_its_command() {
        let p = Params {
            instance: Some(InstanceChoice::Id("1a2b3c4d".to_string())),
            agent: Some("claude".to_string()),
            tool: Some("ripgrep".to_string()),
            packages: vec!["git".to_string(), "jq".to_string()],
            url: Some("http://10.0.0.2:8000".to_string()),
            key: Some(Secret::new("k".to_string())),
            model: Some("m1".to_string()),
            project: Some("/tmp/p".to_string()),
        };
        let expect: &[(Action, &[&[&str]])] = &[
            (Action::Status, &[&["status"]]),
            (Action::StopOne, &[&["stop", "1a2b3c4d"]]),
            (Action::StopAll, &[&["stop"]]),
            (Action::Logs, &[&["logs", "1a2b3c4d"]]),
            (Action::UpdateCheck, &[&["update", "--check"]]),
            (Action::UpdateAll, &[&["update"]]),
            (Action::UpdateAgent, &[&["update", "--agent", "claude"]]),
            (Action::Rollback, &[&["rollback", "--agent", "claude"]]),
            (
                Action::AuthStatus,
                &[
                    &["auth", "claude", "--status"],
                    &["auth", "antigravity", "--status"],
                    &["auth", "omlx", "--status"],
                ],
            ),
            (Action::AuthClaude, &[&["auth", "claude"]]),
            (Action::AuthAntigravity, &[&["auth", "antigravity"]]),
            (
                Action::AuthOmlx,
                &[&[
                    "auth",
                    "omlx",
                    "--url",
                    "http://10.0.0.2:8000",
                    "--model",
                    "m1",
                ]],
            ),
            (Action::ToolsList, &[&["tools", "list"]]),
            (Action::ToolsAdd, &[&["tools", "add", "--", "git", "jq"]]),
            (
                Action::ToolsRemove,
                &[&["tools", "remove", "--", "ripgrep"]],
            ),
            (Action::Doctor, &[&["doctor"]]),
            (Action::Clean, &[&["clean"]]),
            (Action::CleanImages, &[&["clean", "--images"]]),
            (Action::ZedDryRun, &[&["zed", "install", "--dry-run"]]),
            (Action::ZedInstall, &[&["zed", "install"]]),
            (Action::ZedUninstall, &[&["zed", "uninstall"]]),
            (
                Action::Shell,
                &[&["shell", "claude", "--project", "/tmp/p"]],
            ),
            (Action::Setup, &[&["setup"]]),
        ];
        assert_eq!(expect.len(), all_actions(&root()).len());
        for (action, want) in expect {
            let want: Vec<Vec<&str>> = want.iter().map(|w| w.to_vec()).collect();
            assert_eq!(args(&action.invocations(&p)), want, "{action:?}");
        }
        let latest = Params {
            instance: Some(InstanceChoice::Latest),
            ..Params::default()
        };
        assert_eq!(args(&Action::Logs.invocations(&latest)), [["logs"]]);
    }

    #[test]
    fn every_command_is_accepted_by_the_cli_parser() {
        use clap::Parser;
        let p = Params {
            instance: Some(InstanceChoice::Id("1a2b3c4d".to_string())),
            agent: Some("omlx".to_string()),
            tool: Some("ripgrep".to_string()),
            packages: vec!["git".to_string(), "jq".to_string()],
            url: Some("http://10.0.0.2:8000".to_string()),
            key: Some(Secret::new("k".to_string())),
            model: Some("m1".to_string()),
            project: Some("/tmp/p".to_string()),
        };
        for action in all_actions(&root()) {
            for inv in action.invocations(&p) {
                let argv = std::iter::once("zevbox".to_string()).chain(inv.args.clone());
                let parsed = crate::cli::Cli::try_parse_from(argv);
                assert!(parsed.is_ok(), "{action:?}: {:?}", parsed.err());
            }
        }
        // `--` keeps a name like `-x` from being read as an option.
        let parsed = crate::cli::Cli::try_parse_from(["zevbox", "tools", "add", "--", "-x"]);
        assert!(matches!(
            parsed.map(|c| c.command),
            Ok(crate::cli::Command::Tools {
                action: crate::cli::ToolsAction::Add { packages }
            }) if packages == ["-x"]
        ));
    }

    #[test]
    fn values_starting_with_a_dash_are_refused() {
        assert!(TextField::Packages.check("git -y").is_err());
        assert!(TextField::Packages.check("git jq").is_ok());
        assert!(TextField::Url.check("--help").is_err());
        assert!(TextField::Model.check(" -m").is_err());
        assert!(TextField::Project.check("-rf").is_err());
        assert!(TextField::Project.check("/tmp/a-b").is_ok());
        assert!(TextField::Key.check("-key-on-stdin").is_ok());
    }

    #[test]
    fn first_login_of_an_uninstalled_agent_says_it_builds() {
        let mut s = snap(&[]);
        assert_eq!(Action::AuthOmlx.cost_note(&s, Lang::Fr), None);
        s.agents.iter_mut().for_each(|a| a.version = None);
        for a in [
            Action::AuthClaude,
            Action::AuthAntigravity,
            Action::AuthOmlx,
        ] {
            assert!(a
                .cost_note(&s, Lang::Fr)
                .is_some_and(|n| n.contains("construit")));
        }
        assert_eq!(Action::Doctor.cost_note(&s, Lang::Fr), None);
    }

    #[test]
    fn every_entry_has_a_full_description() {
        let root = root();
        for entry in &root.children {
            if let Entry::Menu(m) = entry {
                assert!(
                    Lang::Fr.t(m.description).chars().count() >= 150,
                    "{:?}",
                    m.label
                );
                let panel = menu_panel(m, Lang::Fr);
                assert_eq!(panel.len(), 3 + m.children.len());
            }
        }
        for a in all_actions(&root) {
            for lang in [Lang::En, Lang::Fr] {
                let n = a.description(lang).chars().count();
                assert!((100..=450).contains(&n), "{a:?} {lang:?}: {n} chars");
                assert!(!a.label(lang).is_empty());
            }
            assert_ne!(a.label(Lang::En), a.label(Lang::Fr), "{a:?}");
        }
        let en = Action::UpdateAll.panel(&Params::default(), &snap(&[]), Lang::En);
        assert_eq!(
            en.last(),
            Some(&(Tone::Dim, "Command: zevbox update".to_string()))
        );
        assert_eq!(
            Action::AuthStatus.preview(&Params::default(), Lang::En),
            "zevbox auth <agent> --status (for each agent)"
        );
    }

    #[test]
    fn leaf_panel_ends_with_the_command_and_shows_the_connection() {
        let p = Params::default();
        let panel = Action::UpdateAll.panel(&p, &snap(&[]), Lang::Fr);
        assert_eq!(
            panel.last(),
            Some(&(Tone::Dim, "Commande : zevbox update".to_string()))
        );
        assert!(panel
            .iter()
            .any(|(t, l)| *t == Tone::Dim && l.contains("réseau")));
        let connected = Action::AuthClaude.panel(&p, &snap(&["claude"]), Lang::Fr);
        assert!(connected.contains(&(Tone::Good, "État : connecté".to_string())));
        let not = Action::AuthClaude.panel(&p, &snap(&[]), Lang::Fr);
        assert!(not.contains(&(Tone::Bad, "État : non connecté".to_string())));
        assert!(!Action::Doctor
            .panel(&p, &snap(&[]), Lang::Fr)
            .iter()
            .any(|(_, l)| l.starts_with("État")));
    }

    #[test]
    fn launch_init_and_hidden_options_are_never_offered() {
        let p = Params::default();
        for action in all_actions(&root()) {
            for inv in action.invocations(&p) {
                let first = inv.args.first().map(String::as_str);
                assert_ne!(first, Some("launch"));
                assert_ne!(first, Some("init"));
                assert!(!inv.args.iter().any(|a| a == "--path" || a == "--settings"));
            }
        }
    }

    #[test]
    fn interactive_leaves_are_shell_auth_and_setup() {
        let interactive: Vec<_> = all_actions(&root())
            .into_iter()
            .filter(|a| a.is_interactive())
            .collect();
        assert_eq!(
            interactive,
            [
                Action::AuthClaude,
                Action::AuthAntigravity,
                Action::Shell,
                Action::Setup
            ]
        );
        for a in &interactive {
            assert!(a.invocations(&Params::default())[0].interactive);
        }
    }

    #[test]
    fn confirmed_actions_are_the_list_of_section_6() {
        let always: Vec<_> = all_actions(&root())
            .into_iter()
            .filter(|a| a.confirm_rule() == ConfirmRule::Always)
            .collect();
        assert_eq!(
            always,
            [
                Action::StopOne,
                Action::StopAll,
                Action::UpdateAll,
                Action::UpdateAgent,
                Action::Rollback,
                Action::ToolsAdd,
                Action::ToolsRemove,
                Action::CleanImages,
                Action::ZedUninstall,
            ]
        );
        for a in [
            Action::Status,
            Action::Logs,
            Action::UpdateCheck,
            Action::AuthStatus,
            Action::ToolsList,
            Action::Doctor,
            Action::Clean,
            Action::ZedDryRun,
            Action::ZedInstall,
            Action::Shell,
            Action::Setup,
        ] {
            assert_eq!(a.confirm_rule(), ConfirmRule::Never, "{a:?}");
            assert!(a
                .confirmation(&Params::default(), &snap(&[]), Lang::Fr)
                .is_none());
        }
    }

    #[test]
    fn login_is_confirmed_only_when_already_connected() {
        let p = Params::default();
        for (action, id) in [
            (Action::AuthClaude, "claude"),
            (Action::AuthAntigravity, "antigravity"),
            (Action::AuthOmlx, "omlx"),
        ] {
            assert!(action.confirmation(&p, &snap(&[]), Lang::Fr).is_none());
            let c = action.confirmation(&p, &snap(&[id]), Lang::Fr);
            assert!(
                c.is_some_and(|c| c.question.contains("déjà connecté")),
                "{action:?}"
            );
        }
    }

    #[test]
    fn confirmation_names_the_consequence() {
        let p = Params::default();
        let c = Action::StopAll.confirmation(&p, &snap(&[]), Lang::Fr);
        assert_eq!(
            c.map(|c| c.consequence),
            Some("Les threads ouverts dans Zed seront coupés.".to_string())
        );
    }

    #[test]
    fn omlx_key_is_on_stdin_only() {
        let p = Params {
            url: Some("http://10.0.0.2:8000".to_string()),
            key: Some(Secret::new("sk-secret-123".to_string())),
            model: Some(String::new()),
            ..Params::default()
        };
        let inv = Action::AuthOmlx.invocations(&p);
        assert_eq!(inv.len(), 1);
        assert!(!inv[0].args.iter().any(|a| a.contains("sk-secret-123")));
        assert!(!inv[0].display().contains("sk-secret-123"));
        assert!(!format!("{inv:?}").contains("sk-secret-123"));
        assert!(!Action::AuthOmlx
            .preview(&p, Lang::Fr)
            .contains("sk-secret-123"));
        assert_eq!(
            inv[0].stdin.as_ref().map(Secret::expose),
            Some("sk-secret-123\n")
        );
        // Empty model: no --model.
        assert!(!inv[0].args.iter().any(|a| a == "--model"));
        // Empty key: one empty line, which `auth omlx` reads as « keep the current key ».
        let keep = Params {
            key: Some(Secret::default()),
            ..p
        };
        let inv = Action::AuthOmlx.invocations(&keep);
        assert_eq!(inv[0].stdin.as_ref().map(Secret::expose), Some("\n"));
    }

    #[test]
    fn preview_shows_placeholders_then_choices() {
        let mut p = Params::default();
        assert_eq!(Action::StopOne.preview(&p, Lang::Fr), "zevbox stop <id>");
        assert_eq!(Action::Logs.preview(&p, Lang::Fr), "zevbox logs [<id>]");
        assert_eq!(
            Action::Shell.preview(&p, Lang::Fr),
            "zevbox shell <agent> --project <projet>"
        );
        p.agent = Some("omlx".to_string());
        assert_eq!(
            Action::UpdateAgent.preview(&p, Lang::Fr),
            "zevbox update --agent omlx"
        );
        assert_eq!(Action::Status.preview(&p, Lang::Fr), "zevbox status");
    }
}

//! Pure menu model of `zevbox console` (docs/SPEC_console.md §5, §6): the tree of
//! menus, what each leaf asks for, the `zevbox …` arguments it builds and whether
//! it needs « Êtes-vous sûr ? ». No I/O here: everything is unit-tested.
//!
//! The console never reimplements a command (§2): a leaf only turns the chosen
//! parameters into the argument vector of the current binary.

use super::state::Snapshot;

/// One entry of a menu level: a submenu (marked `›`) or a leaf (an action).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Menu(Menu),
    Leaf(Action),
}

impl Entry {
    pub fn label(&self) -> &'static str {
        match self {
            Entry::Menu(m) => m.label,
            Entry::Leaf(a) => a.label(),
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Entry::Menu(m) => m.description,
            Entry::Leaf(a) => a.description(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Menu {
    pub label: &'static str,
    pub description: &'static str,
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
    pub fn prompt(self) -> &'static str {
        match self {
            TextField::Packages => "Outils à ajouter (noms séparés par des espaces)",
            TextField::Url => "Adresse du serveur oMLX (ex. http://192.168.1.8:8000)",
            TextField::Key => "Clé d'API (masquée)",
            TextField::Model => "Modèle par défaut (optionnel, Entrée = le plus petit)",
            TextField::Project => "Dossier du projet",
        }
    }

    pub fn masked(self) -> bool {
        self == TextField::Key
    }

    /// Refuses what would reach `zevbox` as an option instead of a value. The key
    /// goes on stdin, never in argv, so it is free.
    pub fn check(self, value: &str) -> Result<(), &'static str> {
        let dash = match self {
            TextField::Key => false,
            TextField::Packages => value.split_whitespace().any(|w| w.starts_with('-')),
            TextField::Url | TextField::Model | TextField::Project => value.trim().starts_with('-'),
        };
        if dash {
            Err("Une valeur ne peut pas commencer par « - ».")
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
    pub fn label(self) -> &'static str {
        match self {
            Action::Status => "Voir les agents en cours",
            Action::StopOne => "Arrêter un agent",
            Action::StopAll => "Arrêter tous les agents",
            Action::Logs => "Lire le journal d'une session",
            Action::UpdateCheck => "Vérifier s'il y a des mises à jour",
            Action::UpdateAll => "Tout mettre à jour",
            Action::UpdateAgent => "Mettre à jour un agent",
            Action::Rollback => "Revenir à la version précédente",
            Action::AuthStatus => "État des connexions",
            Action::AuthClaude => "Se connecter à Claude",
            Action::AuthAntigravity => "Se connecter à Antigravity",
            Action::AuthOmlx => "Connecter oMLX",
            Action::ToolsList => "Voir les outils ajoutés",
            Action::ToolsAdd => "Ajouter un outil",
            Action::ToolsRemove => "Retirer un outil",
            Action::Doctor => "Diagnostic complet",
            Action::Clean => "Faire le ménage",
            Action::CleanImages => "Ménage avec les anciennes images",
            Action::ZedDryRun => "Voir ce qui serait écrit",
            Action::ZedInstall => "Installer les entrées zevbox",
            Action::ZedUninstall => "Retirer les entrées zevbox",
            Action::Shell => "Shell dans une VM",
            Action::Setup => "Première installation",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Action::Status => "Liste les agents lancés par Zed : identifiant, agent, projet, état.",
            Action::StopOne => "Arrête un agent choisi dans la liste : sa VM est arrêtée et supprimée.",
            Action::StopAll => "Arrête tous les agents zevbox en cours et supprime leurs VM.",
            Action::Logs => "Affiche la fin du journal d'une session (noms, chemins, tailles ; jamais le contenu des messages).",
            Action::UpdateCheck => "Compare les versions installées avec celles du registre ACP, sans rien changer.",
            Action::UpdateAll => "Télécharge les nouvelles versions, construit et teste les images, puis les active.",
            Action::UpdateAgent => "Met à jour un seul agent : construction, test, puis activation.",
            Action::Rollback => "Réactive la release précédente d'un agent.",
            Action::AuthStatus => "Indique pour chaque agent s'il est connecté.",
            Action::AuthClaude => "Connexion à votre compte Claude, dans la VM : copiez l'adresse affichée dans le navigateur, puis collez le code.",
            Action::AuthAntigravity => "Connexion à votre compte Google pour Antigravity, dans la VM.",
            Action::AuthOmlx => "Relie l'agent oMLX à votre serveur d'inférence : adresse, clé (masquée) et modèle par défaut.",
            Action::ToolsList => "Liste les paquets apt ajoutés aux images des agents.",
            Action::ToolsAdd => "Ajoute des paquets apt aux images, puis reconstruit et met à jour les agents installés.",
            Action::ToolsRemove => "Retire un paquet apt des images, puis reconstruit et met à jour les agents installés.",
            Action::Doctor => "Vérifie toute l'installation et indique comment corriger chaque problème.",
            Action::Clean => "Supprime les restes : instances orphelines, anciens journaux et sauvegardes.",
            Action::CleanImages => "Comme « Faire le ménage », et supprime aussi les images des anciennes versions.",
            Action::ZedDryRun => "Montre les entrées agent_servers que zevbox écrirait dans les réglages de Zed, sans rien écrire.",
            Action::ZedInstall => "Ajoute ou met à jour les entrées zevbox dans les réglages de Zed (avec sauvegarde).",
            Action::ZedUninstall => "Retire les entrées zevbox des réglages de Zed (avec sauvegarde).",
            Action::Shell => "Ouvre un shell Linux dans une VM jetable identique à celle de l'agent, sur un projet.",
            Action::Setup => "Installation guidée : dossier d'état, images, connexions, entrées Zed. Peut être relancée sans risque.",
        }
    }

    /// « Durée … · réseau … » line, when useful.
    pub fn cost(self) -> Option<&'static str> {
        match self {
            Action::StopOne | Action::StopAll => Some("Durée : quelques secondes par agent"),
            Action::UpdateCheck => Some("Durée : quelques secondes · réseau nécessaire"),
            Action::UpdateAll => Some("Durée : 1 à 3 min par agent · réseau nécessaire"),
            Action::UpdateAgent => Some("Durée : 1 à 3 min · réseau nécessaire"),
            Action::AuthClaude | Action::AuthAntigravity => Some("Interactif · réseau nécessaire"),
            Action::AuthOmlx => Some("Durée : quelques secondes · réseau local nécessaire"),
            Action::ToolsAdd | Action::ToolsRemove => {
                Some("Durée : 1 à 3 min par agent installé · réseau nécessaire")
            }
            Action::Doctor | Action::Clean | Action::CleanImages => {
                Some("Durée : quelques secondes")
            }
            Action::Shell => Some("Interactif · démarre une VM (quelques secondes)"),
            Action::Setup => Some("Interactif · Durée : quelques minutes · réseau nécessaire"),
            _ => None,
        }
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
    pub fn cost_note(self, snap: &Snapshot) -> Option<&'static str> {
        let agent = self.auth_agent()?;
        let installed = snap
            .agents
            .iter()
            .any(|a| a.id == agent && a.version.is_some());
        (!installed).then_some(
            "Pas encore installé : la première connexion construit d'abord son image (quelques minutes, réseau nécessaire)",
        )
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
    pub fn confirmation(self, params: &Params, snap: &Snapshot) -> Option<Confirmation> {
        let display = |id: &str| snap.display_of(id);
        let agent = params.agent.as_deref().unwrap_or("");
        let (question, consequence) = match self.confirm_rule() {
            ConfirmRule::Never => return None,
            ConfirmRule::IfAuthenticated(id) if !snap.is_authenticated(id) => return None,
            ConfirmRule::IfAuthenticated(id) => (
                format!("{} est déjà connecté. Refaire la connexion ?", display(id)),
                "La connexion actuelle sera remplacée.".to_string(),
            ),
            ConfirmRule::Always => match self {
                Action::StopOne => {
                    let id = match &params.instance {
                        Some(InstanceChoice::Id(id)) => id.as_str(),
                        _ => "",
                    };
                    (
                        format!("Arrêter l'agent {id} ?"),
                        "Les threads ouverts dans Zed avec cet agent seront coupés.".to_string(),
                    )
                }
                Action::StopAll => (
                    "Arrêter tous les agents ?".to_string(),
                    "Les threads ouverts dans Zed seront coupés.".to_string(),
                ),
                Action::UpdateAll => (
                    "Tout mettre à jour ?".to_string(),
                    "Les images sont reconstruites (réseau, plusieurs minutes) ; les nouvelles versions servent au prochain lancement depuis Zed.".to_string(),
                ),
                Action::UpdateAgent => (
                    format!("Mettre à jour {} ?", display(agent)),
                    "Son image est reconstruite (réseau, quelques minutes) ; la nouvelle version sert au prochain lancement depuis Zed.".to_string(),
                ),
                Action::Rollback => (
                    format!("Revenir à la version précédente de {} ?", display(agent)),
                    "Le prochain lancement depuis Zed utilisera la version précédente.".to_string(),
                ),
                Action::ToolsAdd => (
                    format!("Ajouter {} ?", params.packages.join(" ")),
                    "Les images sont reconstruites et les agents installés mis à jour (réseau, plusieurs minutes).".to_string(),
                ),
                Action::ToolsRemove => (
                    format!("Retirer {} ?", params.tool.as_deref().unwrap_or("")),
                    "Les images sont reconstruites et les agents installés mis à jour (réseau, plusieurs minutes).".to_string(),
                ),
                Action::CleanImages => (
                    "Supprimer les anciennes images ?".to_string(),
                    "Les images des anciennes versions sont effacées : revenir à l'une d'elles demandera de la reconstruire.".to_string(),
                ),
                Action::ZedUninstall => (
                    "Retirer les entrées zevbox de Zed ?".to_string(),
                    "Les agents zevbox disparaissent de Zed (une sauvegarde des réglages est gardée).".to_string(),
                ),
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
    pub fn preview(self, p: &Params) -> String {
        let or = |v: &Option<String>, placeholder: &str| {
            v.clone().unwrap_or_else(|| placeholder.to_string())
        };
        match self {
            Action::StopOne if p.instance.is_none() => "zevbox stop <id>".to_string(),
            Action::Logs if p.instance.is_none() => "zevbox logs [<id>]".to_string(),
            Action::UpdateAgent if p.agent.is_none() => "zevbox update --agent <agent>".to_string(),
            Action::Rollback if p.agent.is_none() => "zevbox rollback --agent <agent>".to_string(),
            Action::AuthStatus => "zevbox auth <agent> --status (pour chaque agent)".to_string(),
            Action::AuthOmlx => {
                let model = match &p.model {
                    Some(m) if !m.is_empty() => format!(" --model {m}"),
                    Some(_) => String::new(),
                    None => " [--model <modèle>]".to_string(),
                };
                format!(
                    "zevbox auth omlx --url {}{model} (clé sur l'entrée standard)",
                    or(&p.url, "<adresse>")
                )
            }
            Action::ToolsAdd if p.packages.is_empty() => "zevbox tools add <paquet>…".to_string(),
            Action::ToolsRemove if p.tool.is_none() => "zevbox tools remove <paquet>".to_string(),
            Action::Shell => format!(
                "zevbox shell {} --project {}",
                or(&p.agent, "<agent>"),
                or(&p.project, "<projet>")
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
        label: "zevbox",
        description: "",
        children: vec![
            menu(
                "Agents en cours",
                "Voir, arrêter et suivre les agents que Zed a lancés.",
                vec![
                    leaf(Action::Status),
                    leaf(Action::StopOne),
                    leaf(Action::StopAll),
                    leaf(Action::Logs),
                ],
            ),
            menu(
                "Mises à jour",
                "Nouvelles versions des agents : vérifier, installer, revenir en arrière.",
                vec![
                    leaf(Action::UpdateCheck),
                    leaf(Action::UpdateAll),
                    leaf(Action::UpdateAgent),
                    leaf(Action::Rollback),
                ],
            ),
            menu(
                "Connexion des agents",
                "Connecter chaque agent à son compte (Claude, Antigravity) ou à votre serveur (oMLX).",
                vec![
                    leaf(Action::AuthStatus),
                    leaf(Action::AuthClaude),
                    leaf(Action::AuthAntigravity),
                    leaf(Action::AuthOmlx),
                ],
            ),
            menu(
                "Outils Linux",
                "Paquets apt ajoutés aux images des agents.",
                vec![
                    leaf(Action::ToolsList),
                    leaf(Action::ToolsAdd),
                    leaf(Action::ToolsRemove),
                ],
            ),
            menu(
                "Diagnostic et ménage",
                "Vérifier l'installation et libérer de la place.",
                vec![
                    leaf(Action::Doctor),
                    leaf(Action::Clean),
                    leaf(Action::CleanImages),
                ],
            ),
            menu(
                "Zed",
                "Les entrées zevbox dans les réglages de Zed (agent_servers).",
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
        let labels: Vec<_> = root.children.iter().map(Entry::label).collect();
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
        assert_eq!(Action::AuthOmlx.cost_note(&s), None);
        s.agents.iter_mut().for_each(|a| a.version = None);
        for a in [
            Action::AuthClaude,
            Action::AuthAntigravity,
            Action::AuthOmlx,
        ] {
            assert!(a.cost_note(&s).is_some_and(|n| n.contains("construit")));
        }
        assert_eq!(Action::Doctor.cost_note(&s), None);
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
            assert!(a.confirmation(&Params::default(), &snap(&[])).is_none());
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
            assert!(action.confirmation(&p, &snap(&[])).is_none());
            let c = action.confirmation(&p, &snap(&[id]));
            assert!(
                c.is_some_and(|c| c.question.contains("déjà connecté")),
                "{action:?}"
            );
        }
    }

    #[test]
    fn confirmation_names_the_consequence() {
        let p = Params::default();
        let c = Action::StopAll.confirmation(&p, &snap(&[]));
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
        assert!(!Action::AuthOmlx.preview(&p).contains("sk-secret-123"));
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
        assert_eq!(Action::StopOne.preview(&p), "zevbox stop <id>");
        assert_eq!(Action::Logs.preview(&p), "zevbox logs [<id>]");
        assert_eq!(
            Action::Shell.preview(&p),
            "zevbox shell <agent> --project <projet>"
        );
        p.agent = Some("omlx".to_string());
        assert_eq!(
            Action::UpdateAgent.preview(&p),
            "zevbox update --agent omlx"
        );
        assert_eq!(Action::Status.preview(&p), "zevbox status");
    }
}

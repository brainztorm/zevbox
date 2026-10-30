//! What the console shows about the installation: banner and picker lists
//! (docs/SPEC_console.md §2, §3). Read only: every change goes through a
//! `zevbox …` command; this module never writes.

use crate::paths::Paths;

/// One agent of `agents::AGENTS` as seen on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentState {
    pub id: &'static str,
    pub display: &'static str,
    /// `agentVersion` of the active release lock; `None` = not installed.
    pub version: Option<String>,
    /// The active entry has a `previous` release (rollback possible).
    pub has_previous: bool,
    /// Same rule as `auth --status` (`cli::auth::is_authenticated`).
    pub authenticated: bool,
}

/// One recorded instance (`instances/<id>.json`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceInfo {
    pub id: String,
    pub agent: String,
    pub project: String,
}

/// Everything the console reads, loaded again after each command (§7).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// `config.json` exists.
    pub installed: bool,
    pub agents: Vec<AgentState>,
    pub instances: Vec<InstanceInfo>,
    /// Ids of `logs/<id>.log`, newest first, at most [`MAX_SESSIONS`].
    pub sessions: Vec<String>,
    /// `tools.json` apt packages.
    pub tools: Vec<String>,
    /// Current oMLX server URL (prefill of « Connecter oMLX »).
    pub omlx_url: Option<String>,
    /// Current oMLX default model (prefill too: an empty field would reset it).
    pub omlx_model: Option<String>,
    /// Current directory (prefill of « Shell dans une VM »).
    pub cwd: String,
}

/// Older sessions are still reachable with `zevbox logs <id>` in a terminal.
pub const MAX_SESSIONS: usize = 30;

impl Snapshot {
    /// Reads the state root. Unreadable parts are shown as empty, never fatal:
    /// the console must open even on a broken or missing installation.
    pub fn load(paths: &Paths) -> Self {
        let active = crate::release::load_active(paths).unwrap_or_default();
        let omlx_settings = paths.state_dir("omlx").join(crate::omlx::AUTH_MARKER);
        let agents = crate::agents::AGENTS
            .iter()
            .map(|spec| {
                let entry = active.get(spec.id);
                let lock = entry.and_then(|e| crate::release::load_lock(paths, &e.current).ok());
                AgentState {
                    id: spec.id,
                    display: spec.display,
                    version: lock.as_ref().map(|l| l.agent_version.clone()),
                    has_previous: entry.is_some_and(|e| e.previous.is_some()),
                    // oMLX: its settings file alone says it (written on the Mac,
                    // independent of a release); the others need the lock's marker.
                    authenticated: if spec.id == "omlx" {
                        crate::omlx::is_configured(&omlx_settings)
                    } else {
                        lock.as_ref().is_some_and(|l| {
                            crate::cli::auth::is_authenticated(paths, spec.id, &l.auth_marker)
                        })
                    },
                }
            })
            .collect();
        let instances = crate::instances::list(paths)
            .unwrap_or_default()
            .into_iter()
            .map(|r| InstanceInfo {
                id: r.id,
                agent: r.agent,
                project: r.project.to_string_lossy().into_owned(),
            })
            .collect();
        let tools = crate::tools::load(&paths.tools_file())
            .map(|t| t.apt)
            .unwrap_or_default();
        let omlx_url = crate::omlx::configured_url(&omlx_settings);
        let omlx_model = crate::omlx::configured_model(&omlx_settings);
        let cwd = std::env::current_dir()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            installed: paths.config_file().exists(),
            agents,
            instances,
            sessions: sessions(paths),
            tools,
            omlx_url,
            omlx_model,
            cwd,
        }
    }

    pub fn display_of(&self, id: &str) -> String {
        self.agents
            .iter()
            .find(|a| a.id == id)
            .map_or_else(|| id.to_string(), |a| a.display.to_string())
    }

    pub fn is_authenticated(&self, id: &str) -> bool {
        self.agents.iter().any(|a| a.id == id && a.authenticated)
    }

    /// The top line (§3): active version of each agent and number of instances.
    pub fn banner(&self) -> String {
        if !self.installed {
            return "zevbox n'est pas installé : lancez Première installation".to_string();
        }
        let versions = self
            .agents
            .iter()
            .map(|a| match &a.version {
                Some(v) => format!("{} {v}", a.display),
                None => format!("{} non installé", a.display),
            })
            .collect::<Vec<_>>()
            .join(" · ");
        let running = match self.instances.len() {
            0 => "aucun agent en cours".to_string(),
            1 => "1 agent en cours".to_string(),
            n => format!("{n} agents en cours"),
        };
        format!("zevbox ─ {versions} ─ {running}")
    }
}

/// Session ids that have a log file, newest first.
fn sessions(paths: &Paths) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(paths.logs_dir()) else {
        return Vec::new();
    };
    let mut found: Vec<(std::time::SystemTime, String)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "log") {
                return None;
            }
            let id = path.file_stem()?.to_string_lossy().into_owned();
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, id))
        })
        .collect();
    found.sort_by_key(|a| std::cmp::Reverse(a.0));
    found
        .into_iter()
        .take(MAX_SESSIONS)
        .map(|(_, id)| id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &'static str, display: &'static str, version: Option<&str>) -> AgentState {
        AgentState {
            id,
            display,
            version: version.map(str::to_string),
            has_previous: false,
            authenticated: false,
        }
    }

    #[test]
    fn banner_lists_versions_and_running_count() {
        let mut snap = Snapshot {
            installed: true,
            agents: vec![
                agent("claude", "Claude", Some("0.99.1")),
                agent("antigravity", "Antigravity", None),
            ],
            ..Snapshot::default()
        };
        assert_eq!(
            snap.banner(),
            "zevbox ─ Claude 0.99.1 · Antigravity non installé ─ aucun agent en cours"
        );
        snap.instances = vec![
            InstanceInfo {
                id: "1".to_string(),
                agent: "claude".to_string(),
                project: "/p".to_string(),
            };
            2
        ];
        assert!(snap.banner().ends_with("─ 2 agents en cours"));
    }

    #[test]
    fn banner_without_config_points_to_setup() {
        let snap = Snapshot::default();
        assert_eq!(
            snap.banner(),
            "zevbox n'est pas installé : lancez Première installation"
        );
    }

    #[test]
    fn load_of_an_empty_state_root_is_not_installed() {
        let dir = tempfile::tempdir().unwrap();
        let snap = Snapshot::load(&Paths::new(dir.path()));
        assert!(!snap.installed);
        assert_eq!(snap.agents.len(), crate::agents::AGENTS.len());
        assert!(snap.agents.iter().all(|a| a.version.is_none()));
        assert!(snap.instances.is_empty() && snap.sessions.is_empty());
    }

    #[test]
    fn omlx_is_connected_from_its_settings_even_without_a_release() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        let settings = paths.state_dir("omlx").join(crate::omlx::AUTH_MARKER);
        std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
        std::fs::write(
            &settings,
            r#"{"env":{"ANTHROPIC_BASE_URL":"http://10.0.0.2:8000","ANTHROPIC_AUTH_TOKEN":"k","ANTHROPIC_DEFAULT_MODEL":"M"}}"#,
        )
        .unwrap();
        let snap = Snapshot::load(&paths);
        assert!(snap.is_authenticated("omlx"));
        assert!(!snap.is_authenticated("claude"));
        assert_eq!(snap.omlx_url.as_deref(), Some("http://10.0.0.2:8000"));
        assert_eq!(snap.omlx_model.as_deref(), Some("M"));
    }
}

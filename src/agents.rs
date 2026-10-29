//! Static table of supported agents (§5.4). Version-dependent data (commands,
//! auth marker) lives in the release lock (§8.7), not here.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentSpec {
    /// CLI identifier: "claude" | "antigravity" | "omlx".
    pub id: &'static str,
    /// Human name used in Zed entry names.
    pub display: &'static str,
    /// Stage name in `assets/Containerfile`. oMLX reuses the Claude stage: it is
    /// the Claude adapter pointed at a local inference server (DECISIONS 2026-10-29).
    pub build_target: &'static str,
    /// Local image repository; the tag is the release id.
    pub image_repo: &'static str,
    /// `id` expected in the ACP registry manifest.
    pub registry_id: &'static str,
    /// Opt-in agent: `update` and `setup` without `--agent` only handle it once it
    /// has an active release (it needs a server of the user's own, `zevbox auth omlx`).
    pub opt_in: bool,
}

pub const AGENTS: [AgentSpec; 3] = [
    AgentSpec {
        id: "claude",
        display: "Claude",
        build_target: "claude",
        image_repo: "zevbox/claude",
        registry_id: "claude-acp",
        opt_in: false,
    },
    AgentSpec {
        id: "antigravity",
        display: "Antigravity",
        build_target: "antigravity",
        image_repo: "zevbox/antigravity",
        registry_id: "antigravity-acp",
        opt_in: false,
    },
    AgentSpec {
        id: "omlx",
        display: "oMLX",
        build_target: "claude",
        image_repo: "zevbox/omlx",
        registry_id: "claude-acp",
        opt_in: true,
    },
];

pub fn find(id: &str) -> Option<&'static AgentSpec> {
    AGENTS.iter().find(|a| a.id == id)
}

/// Containerfile stage of `agent`; an unknown id is its own stage (old locks).
pub fn build_target(agent: &str) -> &str {
    find(agent).map_or(agent, |a| a.build_target)
}

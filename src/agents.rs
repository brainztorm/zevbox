//! Static table of supported agents (§5.4). Version-dependent data (commands,
//! auth marker) lives in the release lock (§8.7), not here.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentSpec {
    /// CLI identifier: "claude" | "antigravity".
    pub id: &'static str,
    /// Human name used in Zed entry names.
    pub display: &'static str,
    /// Stage name in `assets/Containerfile`.
    pub build_target: &'static str,
    /// Local image repository; the tag is the release id.
    pub image_repo: &'static str,
    /// `id` expected in the ACP registry manifest.
    pub registry_id: &'static str,
}

pub const AGENTS: [AgentSpec; 2] = [
    AgentSpec {
        id: "claude",
        display: "Claude",
        build_target: "claude",
        image_repo: "zevbox/claude",
        registry_id: "claude-acp",
    },
    AgentSpec {
        id: "antigravity",
        display: "Antigravity",
        build_target: "antigravity",
        image_repo: "zevbox/antigravity",
        registry_id: "antigravity-acp",
    },
];

pub fn find(id: &str) -> Option<&'static AgentSpec> {
    AGENTS.iter().find(|a| a.id == id)
}

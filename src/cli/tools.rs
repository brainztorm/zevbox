//! `zevbox tools` (§8.1).

use crate::paths::Paths;
use crate::supervisor::{EXIT_ERROR, EXIT_OK, EXIT_PRECONDITION};

pub async fn run(action: super::ToolsAction) -> anyhow::Result<u8> {
    let paths = Paths::from_env()?;
    if !paths.config_file().exists() {
        crate::ui::err(
            "zevbox is not set up. Fix: run 'zevbox setup' or 'zevbox init' in a macOS terminal.",
        );
        return Ok(EXIT_PRECONDITION);
    }

    match action {
        super::ToolsAction::List => {
            let tools = crate::tools::load(&paths.tools_file()).unwrap_or_default();
            for pkg in &tools.apt {
                crate::ui::out(pkg);
            }
            Ok(EXIT_OK)
        }
        super::ToolsAction::Add { packages } => {
            let mut tools = crate::tools::load(&paths.tools_file()).unwrap_or_default();
            let added = match tools.add(&packages) {
                Ok(a) => a,
                Err(e) => {
                    crate::ui::err(&format!("zevbox: {e}"));
                    return Ok(EXIT_ERROR);
                }
            };
            if added.is_empty() {
                crate::ui::out("no new tools to add");
                return Ok(EXIT_OK);
            }
            crate::tools::save(&paths.tools_file(), &tools)?;
            crate::ui::out(&format!("added: {}", added.join(", ")));

            let active = crate::release::load_active(&paths)?;
            for agent_id in active.keys() {
                let agent_arg = match agent_id.as_str() {
                    "claude" => Some(super::AgentArg::Claude),
                    "antigravity" => Some(super::AgentArg::Antigravity),
                    _ => None,
                };
                if let Some(arg) = agent_arg {
                    let code = super::update::run_update(false, Some(arg)).await?;
                    if code != EXIT_OK {
                        return Ok(code);
                    }
                }
            }
            Ok(EXIT_OK)
        }
        super::ToolsAction::Remove { packages } => {
            let mut tools = crate::tools::load(&paths.tools_file()).unwrap_or_default();
            let removed = tools.remove(&packages);
            if removed.is_empty() {
                crate::ui::out("no matching tools to remove");
                return Ok(EXIT_OK);
            }
            crate::tools::save(&paths.tools_file(), &tools)?;
            crate::ui::out(&format!("removed: {}", removed.join(", ")));

            let active = crate::release::load_active(&paths)?;
            for agent_id in active.keys() {
                let agent_arg = match agent_id.as_str() {
                    "claude" => Some(super::AgentArg::Claude),
                    "antigravity" => Some(super::AgentArg::Antigravity),
                    _ => None,
                };
                if let Some(arg) = agent_arg {
                    let code = super::update::run_update(false, Some(arg)).await?;
                    if code != EXIT_OK {
                        return Ok(code);
                    }
                }
            }
            Ok(EXIT_OK)
        }
    }
}

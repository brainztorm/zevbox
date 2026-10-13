//! `zevbox init` (§8.1).

use crate::config::{default_config, save as save_config};
use crate::fsutil::ensure_private_dir;
use crate::paths::Paths;
use crate::supervisor::{EXIT_ERROR, EXIT_OK};
use crate::tools::{save as save_tools, ToolsFile};
use std::path::PathBuf;

pub async fn run() -> anyhow::Result<u8> {
    let paths = Paths::from_env()?;
    for dir in paths.all_dirs() {
        if let Err(e) = ensure_private_dir(&dir) {
            crate::ui::err(&format!(
                "zevbox: cannot create directory {}: {e}",
                dir.display()
            ));
            return Ok(EXIT_ERROR);
        }
    }

    if !paths.config_file().exists() {
        let container_bin = detect_container_binary();
        let uid = nix::unistd::getuid().as_raw();
        let gid = nix::unistd::getgid().as_raw();
        let config = default_config(container_bin, uid, gid);
        if let Err(e) = save_config(&paths.config_file(), &config) {
            crate::ui::err(&format!("zevbox: cannot write config.json: {e}"));
            return Ok(EXIT_ERROR);
        }
    }

    if !paths.tools_file().exists() {
        let tools = ToolsFile::default();
        if let Err(e) = save_tools(&paths.tools_file(), &tools) {
            crate::ui::err(&format!("zevbox: cannot write tools.json: {e}"));
            return Ok(EXIT_ERROR);
        }
    }

    Ok(EXIT_OK)
}

fn detect_container_binary() -> PathBuf {
    if let Ok(out) = std::process::Command::new("which")
        .arg("container")
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() && s.starts_with('/') {
                return PathBuf::from(s);
            }
        }
    }
    for candidate in ["/opt/homebrew/bin/container", "/usr/local/bin/container"] {
        let p = PathBuf::from(candidate);
        if p.exists() {
            return p;
        }
    }
    PathBuf::from("/opt/homebrew/bin/container")
}

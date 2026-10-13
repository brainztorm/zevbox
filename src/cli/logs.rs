//! `zevbox logs` (§8.1).

use crate::paths::Paths;
use crate::supervisor::{EXIT_ERROR, EXIT_OK, EXIT_PRECONDITION};
use std::path::PathBuf;

pub async fn run(id: Option<String>, path: bool) -> anyhow::Result<u8> {
    let paths = Paths::from_env()?;
    if !paths.config_file().exists() {
        crate::ui::err(
            "zevbox is not set up. Fix: run 'zevbox setup' or 'zevbox init' in a macOS terminal.",
        );
        return Ok(EXIT_PRECONDITION);
    }

    let target_file: PathBuf = match id {
        Some(ref instance_id) => paths.log_file(instance_id),
        None => {
            let logs_dir = paths.logs_dir();
            let mut latest: Option<(std::time::SystemTime, PathBuf)> = None;
            if let Ok(entries) = std::fs::read_dir(&logs_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.extension().is_some_and(|ext| ext == "log") {
                        if let Ok(meta) = p.metadata() {
                            if let Ok(modified) = meta.modified() {
                                if latest.as_ref().is_none_or(|(m, _)| modified > *m) {
                                    latest = Some((modified, p));
                                }
                            }
                        }
                    }
                }
            }
            match latest {
                Some((_, p)) => p,
                None => {
                    crate::ui::err("no logs found");
                    return Ok(EXIT_ERROR);
                }
            }
        }
    };

    if !target_file.exists() {
        crate::ui::err(&format!("log file not found: {}", target_file.display()));
        return Ok(EXIT_ERROR);
    }

    if path {
        crate::ui::out(&target_file.to_string_lossy());
        return Ok(EXIT_OK);
    }

    match std::fs::read_to_string(&target_file) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let start = if lines.len() > 200 {
                lines.len() - 200
            } else {
                0
            };
            for line in &lines[start..] {
                crate::ui::out(line);
            }
            Ok(EXIT_OK)
        }
        Err(e) => {
            crate::ui::err(&format!("failed to read log file: {e}"));
            Ok(EXIT_ERROR)
        }
    }
}

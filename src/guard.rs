//! Project guard for `launch` (EXP-01, decision P1 of 2026-10-16).
//!
//! The container masks `<project>/.zed` and makes an existing `<project>/.git`
//! read-only (`container::launch_args`), but it cannot stop the agent from
//! creating a new `.git` elsewhere in the project, and Zed reads git metadata
//! of every repository it finds, even in Restricted Mode. While the agent runs,
//! this guard watches the project on the Mac and stops the agent as soon as a
//! `.git` entry appears or changes anywhere except the root `.git` that existed
//! at launch. It detects and stops; it does not undo (zevbox never writes to
//! the project), so the user is told which path to review.
//!
//! The decision is pure (`is_guarded_path`); the watcher is a thin wrapper.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

const GIT_DIR: &str = ".git";

/// True if `path` (absolute) is inside `project` and is, or is inside, a `.git`
/// entry other than the root `.git` that existed when the agent started.
pub fn is_guarded_path(project: &Path, root_git_existed: bool, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(project) else {
        return false;
    };
    match relative.components().position(|c| c.as_os_str() == GIT_DIR) {
        None => false,
        Some(0) => !root_git_existed,
        Some(_) => true,
    }
}

/// I13 precondition checked at launch. `<project>/.zed` is masked by a tmpfs: that only
/// works, and only protects, when it is absent or a real folder. As a file the
/// container cannot start (opaque error); as a link to a folder of the project the
/// agent would write through it (EXP-01 follow-up, ARGON review 2.1). `None` = fine.
pub fn project_files_problem(project: &Path) -> Option<String> {
    let zed = project.join(".zed");
    match std::fs::symlink_metadata(&zed) {
        Ok(m) if !m.is_dir() => Some(format!(
            "{} is a file or a link, so zevbox cannot mask it safely (I13). \
             Fix: rename or remove it (a regular .zed folder is fine), then start a new thread.",
            zed.display()
        )),
        _ => None,
    }
}

/// Live watcher; dropping it stops the watch.
pub struct Guard {
    _watcher: notify::RecommendedWatcher,
    tripped: Arc<Mutex<Option<PathBuf>>>,
}

impl Guard {
    /// The first guarded path seen, if the guard fired.
    pub fn tripped(&self) -> Option<PathBuf> {
        self.tripped.lock().ok().and_then(|t| t.clone())
    }
}

/// Starts watching `project` recursively. On the first guarded path, records
/// it and cancels `stop` (the launch's shutdown token).
pub fn start(
    project: &Path,
    root_git_existed: bool,
    stop: CancellationToken,
) -> Result<Guard, notify::Error> {
    use notify::Watcher;

    let tripped: Arc<Mutex<Option<PathBuf>>> = Arc::new(Mutex::new(None));
    let seen = Arc::clone(&tripped);
    let root = project.to_path_buf();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };
        if let Some(hit) = event
            .paths
            .iter()
            .find(|p| is_guarded_path(&root, root_git_existed, p))
        {
            if let Ok(mut slot) = seen.lock() {
                if slot.is_none() {
                    // I9: a path, never file content.
                    tracing::warn!(path = %hit.display(), "project guard: git metadata changed");
                    *slot = Some(hit.clone());
                }
            }
            stop.cancel();
        }
    })?;
    watcher.watch(project, notify::RecursiveMode::Recursive)?;
    Ok(Guard {
        _watcher: watcher,
        tripped,
    })
}

/// Message shown in Zed (stderr of `launch`) when the guard stopped the agent.
pub fn tripped_message(path: &Path) -> String {
    format!(
        "zevbox: stopped the agent: it created or changed git metadata at {}. \
         Git settings can make Zed or git run programs on your Mac. \
         Fix: review and delete that path before using git or reopening this project in Zed.",
        path.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    /// Waits until `token` is cancelled or `secs` elapse.
    async fn cancelled_within(token: &CancellationToken, secs: u64) -> bool {
        tokio::time::timeout(std::time::Duration::from_secs(secs), token.cancelled())
            .await
            .is_ok()
    }

    #[tokio::test]
    async fn real_watch_stops_on_nested_git_only() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        // FSEvents reports canonical paths (/private/var/...).
        let project = std::fs::canonicalize(dir.path())?;
        std::fs::create_dir_all(project.join(".git"))?;
        let stop = CancellationToken::new();
        let guard = start(&project, true, stop.clone())?;
        // Let FSEvents settle, then do ordinary work: must not fire.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        std::fs::create_dir_all(project.join("src"))?;
        std::fs::write(project.join("src/main.rs"), "fn main() {}")?;
        std::fs::write(project.join(".git/index"), "x")?;
        assert!(
            !cancelled_within(&stop, 2).await,
            "ordinary writes must not stop the agent"
        );
        assert!(guard.tripped().is_none());
        // A new repository inside the project: must fire.
        std::fs::create_dir_all(project.join("sub/.git"))?;
        std::fs::write(project.join("sub/.git/config"), "[core]\n")?;
        assert!(
            cancelled_within(&stop, 5).await,
            "nested .git must stop the agent"
        );
        let hit = guard.tripped().unwrap_or_default();
        assert!(
            hit.starts_with(project.join("sub/.git")),
            "{}",
            hit.display()
        );
        Ok(())
    }

    #[test]
    fn zed_must_be_absent_or_a_real_folder() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        assert!(project_files_problem(p).is_none(), "absent");
        std::fs::create_dir(p.join(".zed")).unwrap();
        assert!(project_files_problem(p).is_none(), "real folder");
        std::fs::remove_dir(p.join(".zed")).unwrap();
        std::fs::write(p.join(".zed"), "x").unwrap();
        assert!(project_files_problem(p).unwrap().contains("file or a link"));
        std::fs::remove_file(p.join(".zed")).unwrap();
        std::fs::create_dir(p.join("real")).unwrap();
        std::os::unix::fs::symlink("real", p.join(".zed")).unwrap();
        assert!(project_files_problem(p).is_some(), "link to a folder");
        std::fs::remove_file(p.join(".zed")).unwrap();
        std::os::unix::fs::symlink("nowhere", p.join(".zed")).unwrap();
        assert!(project_files_problem(p).is_some(), "dangling link");
    }

    #[test]
    fn root_git_that_existed_is_ignored() {
        let root = p("/u/proj");
        assert!(!is_guarded_path(&root, true, &p("/u/proj/.git/index")));
        assert!(!is_guarded_path(&root, true, &p("/u/proj/.git")));
    }

    #[test]
    fn root_git_created_during_the_session_is_guarded() {
        let root = p("/u/proj");
        assert!(is_guarded_path(&root, false, &p("/u/proj/.git")));
        assert!(is_guarded_path(&root, false, &p("/u/proj/.git/config")));
    }

    #[test]
    fn nested_git_is_always_guarded() {
        let root = p("/u/proj");
        assert!(is_guarded_path(&root, true, &p("/u/proj/sub/.git")));
        assert!(is_guarded_path(&root, true, &p("/u/proj/a/b/.git/config")));
        assert!(is_guarded_path(
            &root,
            false,
            &p("/u/proj/sub/.git/hooks/x")
        ));
    }

    #[test]
    fn ordinary_files_and_outside_paths_are_ignored() {
        let root = p("/u/proj");
        assert!(!is_guarded_path(&root, true, &p("/u/proj/src/main.rs")));
        assert!(!is_guarded_path(&root, true, &p("/u/proj/.gitignore")));
        assert!(!is_guarded_path(
            &root,
            true,
            &p("/u/proj/.github/workflows/ci.yml")
        ));
        assert!(!is_guarded_path(&root, false, &p("/u/other/.git/config")));
        assert!(!is_guarded_path(&root, true, &p("/u/proj-2/sub/.git")));
    }

    #[test]
    fn message_names_the_path() {
        let m = tripped_message(&p("/u/proj/sub/.git"));
        assert!(m.starts_with("zevbox: stopped the agent"));
        assert!(m.contains("/u/proj/sub/.git"));
    }
}

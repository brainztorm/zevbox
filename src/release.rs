//! Releases: registry resolution, lock files, build, smoke test, promotion,
//! rollback (§8.7).

use crate::fsutil::{ensure_private_dir, sha256_hex, write_atomic};
use crate::paths::Paths;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Stdio;

pub const BASE_IMAGE: &str =
    "node:22-bookworm-slim@sha256:c3de60bf2f9dd0ac6370e6117950ff62d6e339527e7472301c9c78a017978392";

pub const CONTAINERFILE_TEMPLATE: &str = include_str!("../assets/Containerfile");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SmokeTest {
    pub passed: bool,
    pub at: String,
    pub details: String,
}

/// `releases/<id>/lock.json` (§8.7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseLock {
    pub schema_version: u32,
    pub release_id: String,
    pub agent: String,
    pub created_at: String,
    pub registry_manifest_url: String,
    pub registry_manifest_sha256: String,
    pub agent_version: String,
    pub base_image: String,
    pub apt_packages: Vec<String>,
    pub npm_package: Option<String>,
    pub archive_url: Option<String>,
    pub archive_sha256: Option<String>,
    pub agent_command: Vec<String>,
    pub auth_command: Vec<String>,
    /// Path relative to `state/<agent>` whose existence means "authenticated".
    pub auth_marker: String,
    pub image: String,
    pub smoke_test: Option<SmokeTest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActiveEntry {
    pub current: String,
    pub previous: Option<String>,
}

/// `releases/active.json`: agent id → entry.
pub type ActiveReleases = BTreeMap<String, ActiveEntry>;

/// What the registry says for Claude.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeComponent {
    pub version: String,
    /// Exact `name@version` from `distribution.npx.package`.
    pub npm_package: String,
}

/// What the registry says for Antigravity (linux-aarch64 only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AntigravityComponent {
    pub version: String,
    pub archive_url: String,
    pub cmd: String,
    pub args: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ReleaseError {
    #[error("registry schema changed: {0}")]
    RegistrySchema(String),
    #[error("no active {0} release")]
    NoActiveRelease(String),
    #[error("release store error: {0}")]
    Store(String),
    #[error("build error: {0}")]
    Build(String),
    #[error("smoke test failed: {0}")]
    SmokeTestFailed(String),
}

/// Requires `id == "claude-acp"`, `version`, `distribution.npx.package`
/// (which must end with `@<version>`). Ignores `preview`.
pub fn parse_claude_manifest(text: &str) -> Result<ClaudeComponent, ReleaseError> {
    let root: Value = serde_json::from_str(text)
        .map_err(|e| ReleaseError::RegistrySchema(format!("invalid JSON: {e}")))?;

    let id = root
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| ReleaseError::RegistrySchema("missing id field".to_string()))?;
    if id != "claude-acp" {
        return Err(ReleaseError::RegistrySchema(format!(
            "expected id 'claude-acp', got '{id}'"
        )));
    }

    let version = root
        .get("version")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| ReleaseError::RegistrySchema("missing or empty version".to_string()))?;

    let npm_package = root
        .get("distribution")
        .and_then(|d| d.get("npx"))
        .and_then(|n| n.get("package"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ReleaseError::RegistrySchema("missing distribution.npx.package".to_string())
        })?;

    let expected_suffix = format!("@{version}");
    if !npm_package.ends_with(&expected_suffix) {
        return Err(ReleaseError::RegistrySchema(format!(
            "npx package '{npm_package}' does not end with '@{version}'"
        )));
    }

    Ok(ClaudeComponent {
        version: version.to_string(),
        npm_package: npm_package.to_string(),
    })
}

/// Requires `id == "antigravity-acp"`, `version`,
/// `distribution.binary["linux-aarch64"]` with `archive` (https URL) and `cmd`;
/// `args` defaults to empty. Never uses another platform.
pub fn parse_antigravity_manifest(text: &str) -> Result<AntigravityComponent, ReleaseError> {
    let root: Value = serde_json::from_str(text)
        .map_err(|e| ReleaseError::RegistrySchema(format!("invalid JSON: {e}")))?;

    let id = root
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| ReleaseError::RegistrySchema("missing id field".to_string()))?;
    if id != "antigravity-acp" {
        return Err(ReleaseError::RegistrySchema(format!(
            "expected id 'antigravity-acp', got '{id}'"
        )));
    }

    let version = root
        .get("version")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| ReleaseError::RegistrySchema("missing or empty version".to_string()))?;

    let linux_arm = root
        .get("distribution")
        .and_then(|d| d.get("binary"))
        .and_then(|b| b.get("linux-aarch64"))
        .ok_or_else(|| {
            ReleaseError::RegistrySchema("missing distribution.binary['linux-aarch64']".to_string())
        })?;

    let archive_url = linux_arm
        .get("archive")
        .and_then(Value::as_str)
        .ok_or_else(|| ReleaseError::RegistrySchema("missing archive URL".to_string()))?;

    if !archive_url.starts_with("https://") {
        return Err(ReleaseError::RegistrySchema(
            "archive URL must use HTTPS".to_string(),
        ));
    }

    let cmd = linux_arm
        .get("cmd")
        .and_then(Value::as_str)
        .ok_or_else(|| ReleaseError::RegistrySchema("missing cmd".to_string()))?;

    let args = match linux_arm.get("args") {
        Some(Value::Array(arr)) => {
            let mut out = Vec::new();
            for a in arr {
                if let Some(s) = a.as_str() {
                    out.push(s.to_string());
                } else {
                    return Err(ReleaseError::RegistrySchema(
                        "args elements must be strings".to_string(),
                    ));
                }
            }
            out
        }
        _ => Vec::new(),
    };

    Ok(AntigravityComponent {
        version: version.to_string(),
        archive_url: archive_url.to_string(),
        cmd: cmd.to_string(),
        args,
    })
}

/// Missing file → empty map.
pub fn load_active(paths: &Paths) -> Result<ActiveReleases, ReleaseError> {
    let file = paths.active_file();
    match std::fs::read_to_string(&file) {
        Ok(content) => serde_json::from_str(&content)
            .map_err(|e| ReleaseError::Store(format!("invalid active releases JSON: {e}"))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ActiveReleases::new()),
        Err(e) => Err(ReleaseError::Store(format!(
            "failed to read active releases from {}: {e}",
            file.display()
        ))),
    }
}

pub fn save_active(paths: &Paths, active: &ActiveReleases) -> Result<(), ReleaseError> {
    ensure_private_dir(&paths.releases_dir())
        .map_err(|e| ReleaseError::Store(format!("failed to create releases dir: {e}")))?;
    let bytes = serde_json::to_vec_pretty(active)
        .map_err(|e| ReleaseError::Store(format!("failed to serialize active releases: {e}")))?;
    write_atomic(&paths.active_file(), &bytes)
        .map_err(|e| ReleaseError::Store(format!("failed to write active releases: {e}")))
}

pub fn load_lock(paths: &Paths, release_id: &str) -> Result<ReleaseLock, ReleaseError> {
    let file = paths.release_lock_file(release_id);
    match std::fs::read_to_string(&file) {
        Ok(content) => serde_json::from_str(&content)
            .map_err(|e| ReleaseError::Store(format!("invalid lock file {}: {e}", file.display()))),
        Err(e) => Err(ReleaseError::Store(format!(
            "failed to read lock file {}: {e}",
            file.display()
        ))),
    }
}

pub fn save_lock(paths: &Paths, release_id: &str, lock: &ReleaseLock) -> Result<(), ReleaseError> {
    let dir = paths.release_dir(release_id);
    ensure_private_dir(&dir)
        .map_err(|e| ReleaseError::Store(format!("failed to create release dir: {e}")))?;
    let bytes = serde_json::to_vec_pretty(lock)
        .map_err(|e| ReleaseError::Store(format!("failed to serialize lock file: {e}")))?;
    write_atomic(&paths.release_lock_file(release_id), &bytes)
        .map_err(|e| ReleaseError::Store(format!("failed to write lock file: {e}")))
}

/// Lock of the `current` release of `agent`, or `NoActiveRelease`.
pub fn active_lock(paths: &Paths, agent: &str) -> Result<ReleaseLock, ReleaseError> {
    let active = load_active(paths)?;
    let entry = active
        .get(agent)
        .ok_or_else(|| ReleaseError::NoActiveRelease(agent.to_string()))?;
    load_lock(paths, &entry.current)
}

/// Images that disk cleanup must keep (DECISIONS G1): for every agent, the
/// image of its `current` and `previous` releases (from the lock, or the
/// `zevbox/<agent>:<release>` naming if the lock is gone), plus the image of
/// every recorded instance (a running agent keeps its image).
pub fn images_to_keep(paths: &Paths) -> Vec<String> {
    let mut keep = Vec::new();
    for (agent, entry) in load_active(paths).unwrap_or_default() {
        for release in std::iter::once(entry.current).chain(entry.previous) {
            keep.push(match load_lock(paths, &release) {
                Ok(lock) => lock.image,
                Err(_) => format!("zevbox/{agent}:{release}"),
            });
        }
    }
    for record in crate::instances::list(paths).unwrap_or_default() {
        keep.push(record.image);
    }
    keep
}

/// Deletes zevbox images not in [`images_to_keep`], one by one, then removes
/// the image builder VM. Never touches other tools' images (`is_zevbox_image`).
/// Returns the deleted references; errors are logged, never fatal.
pub async fn prune_images(paths: &Paths, cli: &crate::container::ContainerCli) -> Vec<String> {
    let mut deleted = Vec::new();
    let keep = images_to_keep(paths);
    match cli.image_list().await {
        Ok(images) => {
            for reference in crate::container::stale_zevbox_images(&images, &keep) {
                match cli.delete_zevbox_image(&reference).await {
                    Ok(()) => deleted.push(reference),
                    Err(e) => tracing::warn!("cannot delete image {reference}: {e}"),
                }
            }
        }
        Err(e) => tracing::warn!("cannot list images: {e}"),
    }
    if let Err(e) = cli.remove_builder().await {
        tracing::info!("image builder not removed: {e}");
    }
    deleted
}

/// Pure: new `current`, old current becomes `previous`.
pub fn promote(active: &mut ActiveReleases, agent: &str, release_id: &str) {
    let previous = active.get(agent).map(|e| e.current.clone());
    active.insert(
        agent.to_string(),
        ActiveEntry {
            current: release_id.to_string(),
            previous,
        },
    );
}

/// Pure: swaps `current` and `previous`; error if there is no previous.
pub fn rollback(active: &mut ActiveReleases, agent: &str) -> Result<(), ReleaseError> {
    let entry = active
        .get_mut(agent)
        .ok_or_else(|| ReleaseError::NoActiveRelease(agent.to_string()))?;
    let prev = entry.previous.take().ok_or_else(|| {
        ReleaseError::Store(format!("no previous release to rollback for {agent}"))
    })?;
    let old_current = std::mem::replace(&mut entry.current, prev);
    entry.previous = Some(old_current);
    Ok(())
}

/// `<agent>-<YYYYMMDD-HHMMSS>` in UTC.
pub fn new_release_id(agent: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    format!("{agent}-{}", now.format("%Y%m%d-%H%M%S"))
}

/// Downloads manifest from the registry via curl.
pub fn fetch_manifest(registry_id: &str) -> Result<(String, String, String), ReleaseError> {
    let url = format!(
        "https://raw.githubusercontent.com/agentclientprotocol/registry/main/{registry_id}/agent.json"
    );
    let out = std::process::Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--max-time",
            "60",
            &url,
        ])
        .output()
        .map_err(|e| ReleaseError::Store(format!("curl error: {e}")))?;

    if !out.status.success() {
        return Err(ReleaseError::Store(format!(
            "failed to download manifest from {url}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let sha = sha256_hex(text.as_bytes());
    Ok((url, sha, text))
}

/// Prepares the build context for an update (§8.7 step 3).
pub fn prepare_context(
    context_dir: &Path,
    lock: &ReleaseLock,
    container_bin: &Path,
) -> Result<(), ReleaseError> {
    ensure_private_dir(context_dir)
        .map_err(|e| ReleaseError::Store(format!("failed to create context dir: {e}")))?;

    let containerfile_path = context_dir.join("Containerfile");
    write_atomic(&containerfile_path, CONTAINERFILE_TEMPLATE.as_bytes())
        .map_err(|e| ReleaseError::Store(format!("failed to write Containerfile: {e}")))?;

    let release_json = serde_json::to_vec_pretty(lock)
        .map_err(|e| ReleaseError::Store(format!("failed to serialize release.json: {e}")))?;
    write_atomic(&context_dir.join("zevbox-release.json"), &release_json)
        .map_err(|e| ReleaseError::Store(format!("failed to write zevbox-release.json: {e}")))?;

    if lock.agent == "claude" {
        let npm_pkg = lock
            .npm_package
            .as_deref()
            .ok_or_else(|| ReleaseError::Store("missing npmPackage in lock".to_string()))?;
        let package_json = serde_json::json!({
            "private": true,
            "dependencies": {
                "@agentclientprotocol/claude-agent-acp": lock.agent_version
            }
        });
        let pkg_bytes = serde_json::to_vec_pretty(&package_json)
            .map_err(|e| ReleaseError::Store(format!("failed to serialize package.json: {e}")))?;
        write_atomic(&context_dir.join("package.json"), &pkg_bytes)
            .map_err(|e| ReleaseError::Store(format!("failed to write package.json: {e}")))?;

        let out = std::process::Command::new(container_bin)
            .args([
                "run",
                "--rm",
                "--mount",
                &format!("type=bind,source={},target=/w", context_dir.display()),
                "-w",
                "/w",
                &lock.base_image,
                "npm",
                "install",
                "--package-lock-only",
                "--ignore-scripts",
                npm_pkg,
            ])
            .output()
            .map_err(|e| {
                ReleaseError::Build(format!("failed to generate package-lock.json: {e}"))
            })?;

        if !out.status.success() {
            return Err(ReleaseError::Build(format!(
                "npm install --package-lock-only failed: {}",
                String::from_utf8_lossy(&out.stderr)
            )));
        }
    } else if lock.agent == "antigravity" {
        let archive_url = lock
            .archive_url
            .as_deref()
            .ok_or_else(|| ReleaseError::Store("missing archiveUrl in lock".to_string()))?;
        let zip_path = context_dir.join("agy.zip");
        let out = std::process::Command::new("curl")
            .args([
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--max-time",
                "120",
                "-o",
                &zip_path.to_string_lossy(),
                archive_url,
            ])
            .output()
            .map_err(|e| ReleaseError::Store(format!("failed to download antigravity zip: {e}")))?;

        if !out.status.success() {
            return Err(ReleaseError::Build(format!(
                "curl failed for antigravity zip: {}",
                String::from_utf8_lossy(&out.stderr)
            )));
        }
    }

    Ok(())
}

/// Builds the container image via `container build` (§8.7 step 4).
pub fn build_image(
    context_dir: &Path,
    lock: &ReleaseLock,
    container_bin: &Path,
    build_log_path: &Path,
    uid: u32,
    gid: u32,
) -> Result<(), ReleaseError> {
    let mut cmd = std::process::Command::new(container_bin);
    cmd.args([
        "build",
        "-f",
        &context_dir.join("Containerfile").to_string_lossy(),
        "--target",
        &lock.agent,
        "--tag",
        &lock.image,
        "--build-arg",
        &format!("BASE_IMAGE={}", lock.base_image),
        "--build-arg",
        &format!("ZEVBOX_UID={uid}"),
        "--build-arg",
        &format!("ZEVBOX_GID={gid}"),
        "--build-arg",
        &format!("APT_PACKAGES={}", lock.apt_packages.join(" ")),
    ]);

    if let Some(sha) = &lock.archive_sha256 {
        cmd.args(["--build-arg", &format!("AGY_SHA256={sha}")]);
    }
    cmd.arg(context_dir);

    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(build_log_path)
        .map_err(|e| ReleaseError::Store(format!("cannot open build.log: {e}")))?;

    cmd.stdout(Stdio::from(log_file.try_clone().map_err(|e| {
        ReleaseError::Store(format!("cannot clone log file handle: {e}"))
    })?));
    cmd.stderr(Stdio::from(log_file));

    let status = cmd
        .status()
        .map_err(|e| ReleaseError::Build(format!("container build failed to spawn: {e}")))?;

    if !status.success() {
        return Err(ReleaseError::Build(format!(
            "container build failed with exit code {:?}; see {}",
            status.code(),
            build_log_path.display()
        )));
    }

    Ok(())
}

/// Smoke tests the newly built image (§8.7 step 5).
pub fn run_smoke_test(lock: &ReleaseLock, container_bin: &Path) -> Result<SmokeTest, ReleaseError> {
    // 1. uname -s == Linux
    let out = std::process::Command::new(container_bin)
        .args(["run", "--rm", &lock.image, "uname", "-s"])
        .output()
        .map_err(|e| ReleaseError::SmokeTestFailed(format!("failed to run uname: {e}")))?;
    if !out.status.success() || String::from_utf8_lossy(&out.stdout).trim() != "Linux" {
        return Err(ReleaseError::SmokeTestFailed(
            "uname -s was not 'Linux'".to_string(),
        ));
    }

    // 2. dpkg -s for tools
    for pkg in &lock.apt_packages {
        let out = std::process::Command::new(container_bin)
            .args(["run", "--rm", &lock.image, "dpkg", "-s", pkg])
            .output()
            .map_err(|e| {
                ReleaseError::SmokeTestFailed(format!("dpkg test failed for {pkg}: {e}"))
            })?;
        if !out.status.success() {
            return Err(ReleaseError::SmokeTestFailed(format!(
                "package {pkg} is not installed"
            )));
        }
    }

    // 3. /etc/zevbox-release.json
    let out = std::process::Command::new(container_bin)
        .args([
            "run",
            "--rm",
            &lock.image,
            "cat",
            "/etc/zevbox-release.json",
        ])
        .output()
        .map_err(|e| ReleaseError::SmokeTestFailed(format!("failed to read release json: {e}")))?;
    let content = String::from_utf8_lossy(&out.stdout);
    if !content.contains(&lock.release_id) {
        return Err(ReleaseError::SmokeTestFailed(
            "/etc/zevbox-release.json missing releaseId".to_string(),
        ));
    }

    Ok(SmokeTest {
        passed: true,
        at: chrono::Utc::now().to_rfc3339(),
        details: "uname, packages and release-id verified".to_string(),
    })
}

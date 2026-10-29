//! The only module that talks to the `container` CLI (§6.2 rule 6, §8.2, I8).
//! Always parse `--format json`, never tables. Never pass `--all`/`-a`.
//!
//! WORKED EXAMPLE (step 6, complete): pure argument builders and parsers first,
//! then a thin async wrapper where every destructive call re-checks its target.

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Stdio;

pub const NAME_PREFIX: &str = "zevbox-";
pub const LABEL_MANAGED: &str = "zevbox.managed";
pub const LABEL_INSTANCE: &str = "zevbox.instance";
pub const LABEL_AGENT: &str = "zevbox.agent";
pub const LABEL_INTERACTIVE: &str = "zevbox.interactive";

/// Everything needed to build a `container run` argument vector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSpec {
    /// `zevbox-<agent>-<id8>` (ACP) or `zevbox-<agent>-sh-<id8>` (interactive).
    pub name: String,
    pub instance_id: String,
    pub agent: String,
    pub cpus: u32,
    pub memory: String,
    pub uid: u32,
    pub gid: u32,
    /// Canonical project path, mounted at the same path.
    pub project: PathBuf,
    /// `stateRoot/state/<agent>`, mounted at `/home/agent`.
    pub state_dir: PathBuf,
    /// `repo:tag`.
    pub image: String,
    pub command: Vec<String>,
    /// `-it` + label `zevbox.interactive=1` instead of `-i` (shell/auth only).
    pub interactive: bool,
}

/// Exact vector of §8.2, without the binary itself, starting with "run".
/// Order: run, -i (or -it), --rm, --init, --progress none, --name, labels (managed, instance,
/// agent[, interactive]), --cpus, --memory, --uid, --gid, --cap-drop ALL,
/// -e HOME=/home/agent, project mount, state mount, -w project, image, command…
pub fn run_args(spec: &RunSpec) -> Vec<String> {
    let project = spec.project.to_string_lossy();
    let state = spec.state_dir.to_string_lossy();
    let mut args: Vec<String> = Vec::new();
    let mut push = |items: &[&str]| args.extend(items.iter().map(|s| s.to_string()));
    // Never -t for ACP: a TTY would mangle the JSON stream (§8.2).
    push(&[
        "run",
        if spec.interactive { "-it" } else { "-i" },
        "--rm",
        "--init",
    ]);
    push(&["--progress", "none", "--name", &spec.name]);
    push(&["--label", &format!("{LABEL_MANAGED}=1")]);
    push(&["--label", &format!("{LABEL_INSTANCE}={}", spec.instance_id)]);
    push(&["--label", &format!("{LABEL_AGENT}={}", spec.agent)]);
    if spec.interactive {
        push(&["--label", &format!("{LABEL_INTERACTIVE}=1")]);
    }
    push(&["--cpus", &spec.cpus.to_string(), "--memory", &spec.memory]);
    push(&[
        "--uid",
        &spec.uid.to_string(),
        "--gid",
        &spec.gid.to_string(),
    ]);
    push(&["--cap-drop", "ALL", "-e", "HOME=/home/agent"]);
    // I1: exactly two mounts. The project keeps its absolute path (ACP cwd, §4).
    push(&[
        "--mount",
        &format!("type=bind,source={project},target={project}"),
    ]);
    push(&[
        "--mount",
        &format!("type=bind,source={state},target=/home/agent"),
    ]);
    push(&["-w", &project, &spec.image]);
    args.extend(spec.command.iter().cloned());
    args
}

/// `run_args` for an ACP launch, plus the project-file protections of EXP-01
/// (decision P1, `docs/DECISIONS.md` 2026-10-16), inserted after `-w project`:
/// `<project>/.zed` is masked by a tmpfs (agent writes never reach the Mac,
/// whether the folder exists or not) and an existing `<project>/.git` is made
/// read-only (the agent can read history but not change git settings, hooks or
/// commit). A missing `.git` is left to the project guard (`guard.rs`).
pub fn launch_args(spec: &RunSpec, git_exists: bool) -> Vec<String> {
    let project = spec.project.to_string_lossy();
    let mut protections = vec!["--tmpfs".to_string(), format!("{project}/.zed")];
    if git_exists {
        protections.push("--read-only-path".to_string());
        protections.push(format!("{project}/.git"));
    }
    let mut args = run_args(spec);
    // "-w <project>" is always present in run_args; insert right after it.
    let at = args
        .windows(2)
        .position(|w| w[0] == "-w")
        .map(|i| i + 2)
        .unwrap_or(args.len());
    args.splice(at..at, protections);
    args
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerInfo {
    /// `configuration.id`.
    pub name: String,
    /// `configuration.labels` (missing → empty).
    pub labels: BTreeMap<String, String>,
    /// `status.state` ("running", "stopped", ...); missing → "unknown".
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageInfo {
    /// `configuration.name`, e.g. "zevbox/claude:claude-20261013-101500".
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemStatus {
    /// `status` == "running".
    pub running: bool,
    /// `client.version`.
    pub version: String,
    /// `host.architecture`.
    pub architecture: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ContainerError {
    #[error("refusing to touch container '{0}': not a zevbox-managed container")]
    Refused(String),
    #[error("cannot run {binary}: {reason}")]
    Spawn { binary: PathBuf, reason: String },
    #[error("'container {command}' failed (exit {code:?}): {stderr}")]
    Failed {
        command: String,
        code: Option<i32>,
        stderr: String,
    },
    #[error("unexpected JSON from 'container': {0}")]
    BadJson(String),
}

/// Parses `container list --all --format json`.
pub fn parse_list_json(text: &str) -> Result<Vec<ContainerInfo>, ContainerError> {
    json_array(text)?
        .iter()
        .map(|c| {
            let name = str_at(c, "/configuration/id")
                .ok_or_else(|| ContainerError::BadJson("missing configuration.id".into()))?;
            let labels = c
                .pointer("/configuration/labels")
                .and_then(Value::as_object)
                .map(|o| {
                    o.iter()
                        .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string())))
                        .collect()
                })
                .unwrap_or_default();
            let state = str_at(c, "/status/state").unwrap_or_else(|| "unknown".into());
            Ok(ContainerInfo {
                name,
                labels,
                state,
            })
        })
        .collect()
}

/// Parses `container image list --format json`.
pub fn parse_image_list_json(text: &str) -> Result<Vec<ImageInfo>, ContainerError> {
    json_array(text)?
        .iter()
        .map(|i| {
            str_at(i, "/configuration/name")
                .map(|reference| ImageInfo { reference })
                .ok_or_else(|| ContainerError::BadJson("missing configuration.name".into()))
        })
        .collect()
}

/// Parses `container system status --format json`.
pub fn parse_system_status_json(text: &str) -> Result<SystemStatus, ContainerError> {
    let v: Value =
        serde_json::from_str(text).map_err(|e| ContainerError::BadJson(e.to_string()))?;
    let status =
        str_at(&v, "/status").ok_or_else(|| ContainerError::BadJson("missing status".into()))?;
    Ok(SystemStatus {
        running: status == "running",
        version: str_at(&v, "/client/version").unwrap_or_default(),
        architecture: str_at(&v, "/host/architecture").unwrap_or_default(),
    })
}

/// Minimum `container` version zevbox supports (plan §8.6, F8).
pub const MIN_CONTAINER_VERSION: (u32, u32, u32) = (1, 5, 0);

/// `Some(true)` if `version` ("1.5.0", "1.6.2-rc1"…) is at least `min`; `None` if it
/// has no leading `major.minor[.patch]` numbers to compare.
pub fn version_at_least(version: &str, min: (u32, u32, u32)) -> Option<bool> {
    let mut parts = version
        .trim()
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<u32>().ok());
    let major = parts.next()??;
    let minor = parts.next().flatten().unwrap_or(0);
    let patch = parts.next().flatten().unwrap_or(0);
    Some((major, minor, patch) >= min)
}

/// Name starts with `zevbox-`, has more characters after it, and contains only
/// `[a-z0-9-]`.
pub fn is_zevbox_name(name: &str) -> bool {
    name.strip_prefix(NAME_PREFIX).is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    })
}

/// I8: `is_zevbox_name` AND label `zevbox.managed == "1"`.
pub fn may_target(info: &ContainerInfo) -> bool {
    // I8: the name alone is not proof (anyone can name a container zevbox-x).
    is_zevbox_name(&info.name) && info.labels.get(LABEL_MANAGED).map(String::as_str) == Some("1")
}

/// `container image list` prints fully-qualified names ("docker.io/library/debian:bookworm-slim",
/// "docker.io/zevbox/claude:<tag>"). True if both designate the same image once a leading
/// "docker.io/" and then a leading "library/" are removed from each.
pub fn image_matches(reference: &str, wanted: &str) -> bool {
    let norm = |s: &str| {
        let s = s.strip_prefix("docker.io/").unwrap_or(s);
        s.strip_prefix("library/").unwrap_or(s).to_string()
    };
    norm(reference) == norm(wanted)
}

/// Repository prefix of every image zevbox builds (`zevbox/<agent>:<release>`).
const ZEVBOX_IMAGE_REPO: &str = "zevbox/";

/// True if `reference` is an image zevbox built, in either form shown by the CLI
/// (`zevbox/claude:t` or `docker.io/zevbox/claude:t`). Images of other tools
/// (kiac, the builder, base images) are never zevbox's to delete.
pub fn is_zevbox_image(reference: &str) -> bool {
    let bare = reference.strip_prefix("docker.io/").unwrap_or(reference);
    bare.strip_prefix(ZEVBOX_IMAGE_REPO)
        .is_some_and(|rest| rest.contains(':') && !rest.contains('/'))
}

/// Disk cleanup (DECISIONS G1): zevbox images present in `images` that match
/// none of `keep` (images of the current and previous releases, and of running
/// instances). Pure; deletion is `ContainerCli::delete_zevbox_image`.
pub fn stale_zevbox_images(images: &[ImageInfo], keep: &[String]) -> Vec<String> {
    images
        .iter()
        .map(|i| i.reference.clone())
        .filter(|r| is_zevbox_image(r) && !keep.iter().any(|k| image_matches(r, k)))
        .collect()
}

/// `container system df --format json` → bytes used by all images (zevbox's and others').
pub fn parse_images_bytes(text: &str) -> Option<u64> {
    serde_json::from_str::<Value>(text)
        .ok()?
        .pointer("/images/sizeInBytes")?
        .as_u64()
}

/// 8 lowercase hex characters from a UUID v4.
pub fn new_instance_id() -> String {
    let mut id = uuid::Uuid::new_v4().simple().to_string();
    id.truncate(8);
    id
}

fn json_array(text: &str) -> Result<Vec<Value>, ContainerError> {
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Array(items)) => Ok(items),
        Ok(_) => Err(ContainerError::BadJson("expected a JSON array".into())),
        Err(e) => Err(ContainerError::BadJson(e.to_string())),
    }
}

fn str_at(v: &Value, pointer: &str) -> Option<String> {
    v.pointer(pointer).and_then(Value::as_str).map(String::from)
}

/// Thin async wrapper over the CLI. Every method logs the command (not secrets)
/// with `tracing::debug!`.
#[derive(Debug, Clone)]
pub struct ContainerCli {
    pub binary: PathBuf,
}

impl ContainerCli {
    pub fn new(binary: PathBuf) -> Self {
        Self { binary }
    }

    /// Runs one CLI command to completion; stdout on success, `Failed` otherwise.
    async fn exec(&self, args: &[&str]) -> Result<String, ContainerError> {
        tracing::debug!(?args, "container");
        let out = tokio::process::Command::new(&self.binary)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|e| ContainerError::Spawn {
                binary: self.binary.clone(),
                reason: e.to_string(),
            })?;
        if !out.status.success() {
            return Err(ContainerError::Failed {
                command: args.join(" "),
                code: out.status.code(),
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            });
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    pub async fn list_all(&self) -> Result<Vec<ContainerInfo>, ContainerError> {
        // `--all` here only *reads*; it is never used with stop/delete (I8).
        parse_list_json(&self.exec(&["list", "--all", "--format", "json"]).await?)
    }

    pub async fn image_list(&self) -> Result<Vec<ImageInfo>, ContainerError> {
        parse_image_list_json(&self.exec(&["image", "list", "--format", "json"]).await?)
    }

    pub async fn system_status(&self) -> Result<SystemStatus, ContainerError> {
        parse_system_status_json(&self.exec(&["system", "status", "--format", "json"]).await?)
    }

    pub async fn system_start(&self) -> Result<(), ContainerError> {
        self.exec(&["system", "start"]).await.map(|_| ())
    }

    /// `container stop --time <secs> <name>`. Returns `Refused` without running
    /// anything if `!is_zevbox_name(name)`.
    pub async fn stop(&self, name: &str, secs: u32) -> Result<(), ContainerError> {
        if !is_zevbox_name(name) {
            return Err(ContainerError::Refused(name.to_string()));
        }
        self.exec(&["stop", "--time", &secs.to_string(), name])
            .await
            .map(|_| ())
    }

    /// `container delete --force <name>`. Same refusal rule as `stop`.
    pub async fn delete_force(&self, name: &str) -> Result<(), ContainerError> {
        if !is_zevbox_name(name) {
            return Err(ContainerError::Refused(name.to_string()));
        }
        self.exec(&["delete", "--force", name]).await.map(|_| ())
    }

    /// `container image delete <reference>`, only for an image zevbox built
    /// (never `--all`, never another tool's image).
    pub async fn delete_zevbox_image(&self, reference: &str) -> Result<(), ContainerError> {
        if !is_zevbox_image(reference) {
            return Err(ContainerError::Refused(reference.to_string()));
        }
        self.exec(&["image", "delete", reference]).await.map(|_| ())
    }

    /// Stops then deletes the image builder VM (`container builder`), which keeps
    /// a disk that never shrinks between builds. Recreated by the next build.
    pub async fn remove_builder(&self) -> Result<(), ContainerError> {
        let _ = self.exec(&["builder", "stop"]).await;
        self.exec(&["builder", "delete"]).await.map(|_| ())
    }

    /// HTTP status of `GET <url>` seen from a throw-away VM of `image` (no mount,
    /// no secret: a 401 still proves the server is reachable). "000" = unreachable.
    /// The VM network only reaches the Mac's primary interface (DECISIONS 2026-10-29).
    pub async fn http_status_from_vm(
        &self,
        image: &str,
        url: &str,
    ) -> Result<String, ContainerError> {
        let out = self
            .exec(&[
                "run",
                "--rm",
                "--progress",
                "none",
                image,
                "curl",
                "--silent",
                "--output",
                "/dev/null",
                "--max-time",
                "5",
                "--write-out",
                "%{http_code}",
                url,
            ])
            .await;
        match out {
            Ok(code) => Ok(code.trim().to_string()),
            // curl exits non-zero when it cannot connect; it still printed "000".
            Err(ContainerError::Failed { code: Some(_), .. }) => Ok("000".to_string()),
            Err(e) => Err(e),
        }
    }

    /// Bytes used by all images (`container system df`), if the CLI reports it.
    pub async fn images_bytes(&self) -> Option<u64> {
        parse_images_bytes(
            &self
                .exec(&["system", "df", "--format", "json"])
                .await
                .ok()?,
        )
    }

    /// Looks the container up with `list_all`, then stops and deletes it only if
    /// `may_target` is true. Ok(false) if it does not exist.
    pub async fn stop_and_delete_verified(&self, name: &str) -> Result<bool, ContainerError> {
        let Some(info) = self.list_all().await?.into_iter().find(|c| c.name == name) else {
            return Ok(false);
        };
        if !may_target(&info) {
            return Err(ContainerError::Refused(name.to_string()));
        }
        if info.state == "running" {
            // A failed stop is not fatal: delete --force below still removes it.
            if let Err(e) = self.stop(name, 3).await {
                tracing::warn!("stop {name}: {e}");
            }
        }
        // `--rm` removal is asynchronous (F15): delete only if still listed.
        if self.list_all().await?.iter().any(|c| c.name == name) {
            self.delete_force(name).await?;
        }
        Ok(true)
    }
}

#[cfg(test)]
mod launch_args_tests {
    use super::*;

    fn spec() -> RunSpec {
        RunSpec {
            name: "zevbox-claude-0a1b2c3d".into(),
            instance_id: "0a1b2c3d".into(),
            agent: "claude".into(),
            cpus: 4,
            memory: "4G".into(),
            uid: 501,
            gid: 20,
            project: PathBuf::from("/Users/test/code/my project"),
            state_dir: PathBuf::from("/Users/test/state/claude"),
            image: "zevbox/claude:t".into(),
            command: vec!["agent".into()],
            interactive: false,
        }
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn protections_follow_workdir_and_precede_image() {
        let a = launch_args(&spec(), true);
        let w = a.iter().position(|x| x == "-w").unwrap_or(usize::MAX);
        assert_eq!(
            a[w..w + 7],
            s(&[
                "-w",
                "/Users/test/code/my project",
                "--tmpfs",
                "/Users/test/code/my project/.zed",
                "--read-only-path",
                "/Users/test/code/my project/.git",
                "zevbox/claude:t",
            ])[..]
        );
        assert_eq!(a.last().map(String::as_str), Some("agent"));
    }

    #[test]
    fn missing_git_only_masks_zed() {
        let a = launch_args(&spec(), false);
        assert!(a
            .windows(2)
            .any(|w| w == ["--tmpfs", "/Users/test/code/my project/.zed"]));
        assert!(!a.iter().any(|x| x == "--read-only-path"));
    }

    #[test]
    fn keeps_exactly_two_host_mounts() {
        let a = launch_args(&spec(), true);
        assert_eq!(a.iter().filter(|x| *x == "--mount").count(), 2, "I1");
        assert_eq!(a.len(), run_args(&spec()).len() + 4);
    }
}

#[cfg(test)]
mod image_cleanup_tests {
    use super::*;

    fn imgs(refs: &[&str]) -> Vec<ImageInfo> {
        refs.iter()
            .map(|r| ImageInfo {
                reference: r.to_string(),
            })
            .collect()
    }

    #[test]
    fn only_zevbox_images_qualify() {
        assert!(is_zevbox_image("zevbox/claude:claude-1"));
        assert!(is_zevbox_image("docker.io/zevbox/antigravity:a-2"));
        assert!(!is_zevbox_image("docker.io/library/debian:bookworm-slim"));
        assert!(!is_zevbox_image(
            "ghcr.io/apple/container-builder-shim/builder:0.13.1"
        ));
        assert!(!is_zevbox_image("docker.io/rancher/k3s@sha256:abc"));
        assert!(!is_zevbox_image("evil/zevbox/claude:x"));
        assert!(!is_zevbox_image("zevbox/claude"), "no tag");
    }

    #[test]
    fn keeps_current_previous_and_running_images() {
        let images = imgs(&[
            "docker.io/zevbox/claude:c-1",
            "docker.io/zevbox/claude:c-2",
            "docker.io/zevbox/claude:c-3",
            "docker.io/zevbox/antigravity:a-1",
            "docker.io/zevbox/antigravity:a-2",
            "docker.io/library/debian:bookworm-slim",
            "docker.io/rancher/k3s:v1",
        ]);
        let keep: Vec<String> = [
            "zevbox/claude:c-3",
            "zevbox/claude:c-2",
            "zevbox/antigravity:a-2",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            stale_zevbox_images(&images, &keep),
            vec![
                "docker.io/zevbox/claude:c-1".to_string(),
                "docker.io/zevbox/antigravity:a-1".to_string()
            ]
        );
    }

    #[test]
    fn parses_system_df() {
        let j = r#"{"containers":{"sizeInBytes":1},"images":{"active":3,"sizeInBytes":61615472640,"total":30}}"#;
        assert_eq!(parse_images_bytes(j), Some(61_615_472_640));
        assert_eq!(parse_images_bytes("{}"), None);
        assert_eq!(parse_images_bytes("not json"), None);
    }
}

#[cfg(test)]
mod version_tests {
    use super::*;

    #[test]
    fn compares_versions() {
        let min = MIN_CONTAINER_VERSION;
        assert_eq!(version_at_least("1.5.0", min), Some(true));
        assert_eq!(version_at_least("1.5.3", min), Some(true));
        assert_eq!(version_at_least("1.10.0", min), Some(true));
        assert_eq!(version_at_least("2.0", min), Some(true));
        assert_eq!(version_at_least("1.4.9", min), Some(false));
        assert_eq!(version_at_least("0.9.0", min), Some(false));
        assert_eq!(version_at_least("1.5.0-rc1", min), Some(true));
        assert_eq!(version_at_least("", min), None);
        assert_eq!(version_at_least("unknown", min), None);
    }
}

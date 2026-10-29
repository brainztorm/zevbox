//! `zevbox auth omlx` (DECISIONS 2026-10-29): connects the oMLX agent to the
//! user's inference server. Runs on the Mac: asks for the URL and the key, lists
//! the server's models, writes `state/omlx/.claude/settings.json` (0600) and
//! checks that a VM can reach the server.

use crate::omlx::{self, ServerModel};
use crate::paths::Paths;
use crate::supervisor::{EXIT_ERROR, EXIT_OK};
use std::io::{BufRead, IsTerminal, Write};
use std::process::{Command, Stdio};

/// Options of `zevbox auth omlx`; prompted for when missing.
#[derive(Debug, Clone, Default)]
pub struct OmlxOptions {
    pub url: Option<String>,
    pub model: Option<String>,
}

pub async fn run(
    paths: &Paths,
    config: &crate::config::Config,
    lock: &crate::release::ReleaseLock,
    opts: OmlxOptions,
) -> anyhow::Result<u8> {
    let settings_path = paths.state_dir("omlx").join(omlx::AUTH_MARKER);
    let existing: Option<serde_json::Value> = std::fs::read_to_string(&settings_path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    let previous_url = omlx::configured_url(&settings_path);

    let url_input = match opts.url {
        Some(u) => u,
        None => prompt_line(&format!(
            "oMLX server URL{}: ",
            previous_url
                .as_deref()
                .map(|u| format!(" [{u}]"))
                .unwrap_or_else(|| " (e.g. http://192.168.1.8:8000)".to_string())
        ))?
        .filter(|s| !s.trim().is_empty())
        .or(previous_url.clone())
        .unwrap_or_default(),
    };
    let url = match omlx::validate_url(&url_input) {
        Ok(u) => u,
        Err(e) => {
            crate::ui::err(&format!(
                "zevbox: {e}. Fix: run 'zevbox auth omlx --url http://<server>:<port>'"
            ));
            return Ok(EXIT_ERROR);
        }
    };

    let previous_key = existing
        .as_ref()
        .and_then(|s| s.pointer("/env/ANTHROPIC_AUTH_TOKEN"))
        .and_then(|k| k.as_str())
        .filter(|k| !k.is_empty())
        .map(str::to_string);
    let key_input = read_secret(if previous_key.is_some() {
        "API key (hidden; Enter keeps the current one): "
    } else {
        "API key (hidden): "
    })?;
    let key = match key_input.filter(|k| !k.trim().is_empty()).or(previous_key) {
        Some(k) => match omlx::validate_key(&k) {
            Ok(k) => k,
            Err(e) => {
                crate::ui::err(&format!("zevbox: {e}. Fix: run 'zevbox auth omlx' again"));
                return Ok(EXIT_ERROR);
            }
        },
        None => {
            crate::ui::err("zevbox: no API key given. Fix: run 'zevbox auth omlx' in a terminal");
            return Ok(EXIT_ERROR);
        }
    };

    // From the Mac: proves the URL and the key before anything is written.
    let models = match fetch_models(&url, &key) {
        Ok(m) => m,
        Err(e) => {
            crate::ui::err(&format!(
                "zevbox: cannot list the models of {url}: {e}. Fix: check that oMLX is running and the URL and key are right, then run 'zevbox auth omlx' again"
            ));
            return Ok(EXIT_ERROR);
        }
    };
    let Some(helper) = omlx::smallest(&models).map(|m| m.id.clone()) else {
        crate::ui::err("zevbox: the server offers no model. Fix: load a model in oMLX");
        return Ok(EXIT_ERROR);
    };
    let default_model = match opts.model {
        Some(m) if models.iter().any(|s| s.id == m) => m,
        Some(m) => {
            crate::ui::err(&format!(
                "zevbox: model '{m}' is not on the server (available: {}). Fix: run 'zevbox auth omlx --model <one of them>'",
                model_names(&models)
            ));
            return Ok(EXIT_ERROR);
        }
        None => helper.clone(),
    };

    let settings = omlx::settings_json(existing, &url, &key, &default_model, &helper, &models);
    if let Some(dir) = settings_path.parent() {
        crate::fsutil::ensure_private_dir(dir)?;
    }
    let body = format!("{}\n", serde_json::to_string_pretty(&settings)?);
    // New file: the temp file is created 0600; an existing one keeps its mode.
    crate::fsutil::write_atomic(&settings_path, body.as_bytes())?;

    crate::ui::out(&format!(
        "omlx: connected to {url}\n  models: {}\n  default: {default_model}\n  helper (small background requests, never unloaded): {helper}\n  switching model in Zed unloads the previous one on the server",
        model_names(&models)
    ));

    // The agent runs in a VM, whose network only goes out through the Mac's
    // primary interface (DECISIONS 2026-10-29): check what the VM sees.
    let cli = crate::container::ContainerCli::new(config.container_binary.clone());
    match cli
        .http_status_from_vm(&lock.image, &format!("{url}/v1/models"))
        .await
    {
        Ok(code) if code != "000" => {
            crate::ui::out(&format!(
                "omlx: the agent VM reaches the server (HTTP {code})"
            ));
        }
        Ok(_) => {
            crate::ui::err(&format!(
                "zevbox: the Mac reaches {url} but the agent VM does not. The VMs only go out through the Mac's main network (Wi-Fi or Ethernet with the default route), not through a direct cable to the server. Fix: run 'zevbox auth omlx --url http://<server address on the main network>:<port>'"
            ));
            return Ok(EXIT_ERROR);
        }
        Err(e) => crate::ui::err(&format!(
            "omlx: could not test the server from a VM ({e}); the settings are saved"
        )),
    }
    Ok(EXIT_OK)
}

fn model_names(models: &[ServerModel]) -> String {
    models
        .iter()
        .map(|m| m.id.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// `GET /v1/models/status` (oMLX, with sizes), else `GET /v1/models`.
fn fetch_models(url: &str, key: &str) -> anyhow::Result<Vec<ServerModel>> {
    match curl_get(&format!("{url}/v1/models/status"), key)
        .and_then(|text| omlx::parse_models(&text).map_err(anyhow::Error::msg))
    {
        Ok(models) => Ok(models),
        Err(_) => omlx::parse_models(&curl_get(&format!("{url}/v1/models"), key)?)
            .map_err(anyhow::Error::msg),
    }
}

/// curl with the key in a config read from stdin (never in argv, I9).
fn curl_get(url: &str, key: &str) -> anyhow::Result<String> {
    let mut child = Command::new("/usr/bin/curl")
        .args([
            "--config",
            "-",
            "--fail",
            "--silent",
            "--show-error",
            "--max-time",
            "10",
            url,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(omlx::curl_auth_config(key).as_bytes())?;
    }
    let out = child.wait_with_output()?;
    if !out.status.success() {
        anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// One line from stdin; `None` at end of input.
fn prompt_line(prompt: &str) -> anyhow::Result<Option<String>> {
    if std::io::stdin().is_terminal() {
        crate::ui::out_inline(prompt);
    }
    let mut line = String::new();
    let n = std::io::stdin().lock().read_line(&mut line)?;
    Ok((n > 0).then(|| line.trim().to_string()))
}

/// Like [`prompt_line`], without echo on a terminal (`/bin/stty`, no shell, I7).
/// Piped input (`echo "$KEY" | zevbox auth omlx --url …`) is read as is.
fn read_secret(prompt: &str) -> anyhow::Result<Option<String>> {
    if !std::io::stdin().is_terminal() {
        return prompt_line(prompt);
    }
    let stty = |arg: &str| {
        Command::new("/bin/stty")
            .arg(arg)
            .stdin(Stdio::inherit())
            .status()
    };
    let _ = stty("-echo");
    let line = prompt_line(prompt);
    let _ = stty("echo");
    crate::ui::out("");
    line
}

//! `zevbox auth` (§8.1).

use crate::paths::Paths;
use crate::supervisor::{EXIT_ERROR, EXIT_OK, EXIT_PRECONDITION};

pub async fn run(agent: super::AgentArg, status: bool) -> anyhow::Result<u8> {
    let paths = Paths::from_env()?;
    if !paths.config_file().exists() {
        crate::ui::err(
            "zevbox is not set up. Fix: run 'zevbox setup' or 'zevbox init' in a macOS terminal.",
        );
        return Ok(EXIT_PRECONDITION);
    }
    let config = crate::config::load(&paths.config_file())?;

    let lock = match crate::release::active_lock(&paths, agent.id()) {
        Ok(l) => l,
        Err(_) => {
            if status {
                crate::ui::out("unknown");
                return Ok(EXIT_OK);
            } else {
                crate::ui::err(&format!(
                    "no active release for {}. Fix: run 'zevbox update --agent {}'",
                    agent.id(),
                    agent.id()
                ));
                return Ok(EXIT_PRECONDITION);
            }
        }
    };

    let marker_path = paths.state_dir(agent.id()).join(&lock.auth_marker);

    if status {
        if marker_path.exists() {
            crate::ui::out("authenticated");
        } else {
            crate::ui::out("not authenticated");
        }
        return Ok(EXIT_OK);
    }

    // Ensure state dir exists with 0700
    crate::fsutil::ensure_private_dir(&paths.state_dir(agent.id()))?;

    if agent.id() == "antigravity" {
        return auth_antigravity(&paths).await;
    }

    let project = match config.projects.first() {
        Some(p) => p.clone(),
        None => match std::env::current_dir() {
            Ok(p) => p,
            Err(_) => std::path::PathBuf::from("/"),
        },
    };

    let id8 = crate::container::new_instance_id();
    let name = format!("zevbox-{}-sh-{id8}", agent.id());

    let spec = crate::container::RunSpec {
        name,
        instance_id: format!("auth-{id8}"),
        agent: agent.id().to_string(),
        cpus: config.resources.cpus,
        memory: config.resources.memory.clone(),
        uid: config.host_uid,
        gid: config.host_gid,
        project,
        state_dir: paths.state_dir(agent.id()),
        image: lock.image,
        command: lock.auth_command,
        interactive: true,
    };

    let args = crate::container::run_args(&spec);
    let mut cmd = std::process::Command::new(&config.container_binary);
    cmd.args(&args);
    cmd.stdin(std::process::Stdio::inherit());
    cmd.stdout(std::process::Stdio::inherit());
    cmd.stderr(std::process::Stdio::inherit());

    let exit_status = cmd.status()?;
    if exit_status.success() {
        Ok(EXIT_OK)
    } else {
        Ok(EXIT_ERROR)
    }
}

async fn auth_antigravity(paths: &Paths) -> anyhow::Result<u8> {
    const CLIENT_ID: &str =
        "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
    const CLIENT_SECRET: &str = "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";
    const SCOPES: &str = "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/aicode";

    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}/");

    let raw_verifier = format!(
        "{}-{}-{}",
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4()
    );
    let code_verifier = raw_verifier.replace('-', "");

    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(code_verifier.as_bytes());
    let code_challenge = base64_url_encode(&hash);
    let state = uuid::Uuid::new_v4().to_string().replace('-', "");

    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent&code_challenge={}&code_challenge_method=S256&state={}",
        CLIENT_ID,
        url_encode(&redirect_uri),
        url_encode(SCOPES),
        code_challenge,
        state
    );

    crate::ui::out(&format!(
        "Opening browser for Google Antigravity authentication...\nIf the browser does not open automatically, visit:\n{auth_url}"
    ));

    let _ = std::process::Command::new("open").arg(&auth_url).spawn();

    listener.set_nonblocking(true)?;
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(120);

    let mut stream = loop {
        match listener.accept() {
            Ok((s, _)) => break s,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if start.elapsed() > timeout {
                    crate::ui::err("Authentication timed out waiting for browser callback.");
                    return Ok(EXIT_ERROR);
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            Err(e) => return Err(e.into()),
        }
    };

    use std::io::{Read, Write};
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf)?;
    let req = String::from_utf8_lossy(&buf[..n]);

    let code = match extract_query_param(&req, "code") {
        Some(c) => c,
        None => {
            let error_msg = extract_query_param(&req, "error")
                .unwrap_or_else(|| "missing code parameter".to_string());
            let _ = stream.write_all(
                b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n<h1>Authentication Failed</h1>",
            );
            crate::ui::err(&format!("Google authentication failed: {error_msg}"));
            return Ok(EXIT_ERROR);
        }
    };

    let response = b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<!DOCTYPE html><html><body style=\"font-family: sans-serif; text-align: center; padding-top: 50px;\"><h2>Authentification Google r&eacute;ussie !</h2><p>Vous pouvez fermer cet onglet et retourner dans votre terminal.</p></body></html>";
    let _ = stream.write_all(response);
    let _ = stream.flush();

    crate::ui::out("Exchanging authorization code with Google...");

    let token_output = std::process::Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--max-time",
            "30",
            "--data-urlencode",
            &format!("client_id={CLIENT_ID}"),
            "--data-urlencode",
            &format!("client_secret={CLIENT_SECRET}"),
            "--data-urlencode",
            &format!("code={code}"),
            "--data-urlencode",
            &format!("code_verifier={code_verifier}"),
            "--data-urlencode",
            "grant_type=authorization_code",
            "--data-urlencode",
            &format!("redirect_uri={redirect_uri}"),
            "https://oauth2.googleapis.com/token",
        ])
        .output()?;

    if !token_output.status.success() {
        crate::ui::err(&format!(
            "Failed to exchange authorization code for tokens: {}",
            String::from_utf8_lossy(&token_output.stderr).trim()
        ));
        return Ok(EXIT_ERROR);
    }

    let token_resp: serde_json::Value = serde_json::from_slice(&token_output.stdout)?;
    let refresh_token = match token_resp.get("refresh_token").and_then(|v| v.as_str()) {
        Some(rt) => rt.to_string(),
        None => {
            crate::ui::err("Google OAuth response did not contain a refresh token.");
            return Ok(EXIT_ERROR);
        }
    };

    let state_dir = paths.state_dir("antigravity");
    let gemini_dir = state_dir.join(".gemini");
    let acp_dir = gemini_dir.join("antigravity-acp");
    crate::fsutil::ensure_private_dir(&gemini_dir)?;
    crate::fsutil::ensure_private_dir(&acp_dir)?;

    let settings_json = serde_json::json!({
        "auth": {
            "type": "oauth-personal"
        }
    });
    crate::fsutil::write_atomic(
        &acp_dir.join("settings.json"),
        serde_json::to_string_pretty(&settings_json)?.as_bytes(),
    )?;

    let acp_token_json = serde_json::json!({
        "client_id": CLIENT_ID,
        "client_secret": CLIENT_SECRET,
        "refresh_token": refresh_token,
        "token_uri": "https://oauth2.googleapis.com/token",
        "scopes": [
            "https://www.googleapis.com/auth/cloud-platform",
            "https://www.googleapis.com/auth/userinfo.email",
            "https://www.googleapis.com/auth/aicode"
        ],
        "project_id": ""
    });
    crate::fsutil::write_atomic(
        &acp_dir.join("acp_token.json"),
        serde_json::to_string_pretty(&acp_token_json)?.as_bytes(),
    )?;

    crate::ui::out("Google Antigravity authenticated successfully.");
    Ok(EXIT_OK)
}

fn base64_url_encode(data: &[u8]) -> String {
    const URL_SAFE_CHARS: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut res = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
        res.push(URL_SAFE_CHARS[(b0 >> 2) as usize] as char);
        res.push(URL_SAFE_CHARS[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            res.push(URL_SAFE_CHARS[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        }
        if chunk.len() > 2 {
            res.push(URL_SAFE_CHARS[(b2 & 0x3f) as usize] as char);
        }
    }
    res
}

fn url_encode(input: &str) -> String {
    let mut out = String::new();
    for b in input.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn extract_query_param(req: &str, param: &str) -> Option<String> {
    let first_line = req.lines().next()?;
    let path = first_line.split_whitespace().nth(1)?;
    let query = path.split('?').nth(1)?;
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next()?;
        if key == param {
            let val = parts.next().unwrap_or("");
            return Some(percent_decode(val));
        }
    }
    None
}

fn percent_decode(input: &str) -> String {
    let mut bytes = Vec::new();
    let mut chars = input.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let h1 = chars.next().unwrap_or(0);
            let h2 = chars.next().unwrap_or(0);
            let hex_str = format!("{}{}", h1 as char, h2 as char);
            if let Ok(byte) = u8::from_str_radix(&hex_str, 16) {
                bytes.push(byte);
            }
        } else if b == b'+' {
            bytes.push(b' ');
        } else {
            bytes.push(b);
        }
    }
    String::from_utf8_lossy(&bytes).to_string()
}

//! `zevbox auth` (§8.1).

use crate::paths::Paths;
use crate::supervisor::{EXIT_ERROR, EXIT_OK, EXIT_PRECONDITION};

pub use super::auth_omlx::OmlxOptions;

pub async fn run(agent: super::AgentArg, status: bool) -> anyhow::Result<u8> {
    run_with(agent, status, OmlxOptions::default()).await
}

/// `zevbox auth <agent> [--status] [--url U] [--model M]` (the options are oMLX's).
pub async fn run_with(
    agent: super::AgentArg,
    status: bool,
    omlx: OmlxOptions,
) -> anyhow::Result<u8> {
    if agent != super::AgentArg::Omlx && (omlx.url.is_some() || omlx.model.is_some()) {
        crate::ui::err("zevbox: --url and --model only apply to 'zevbox auth omlx'");
        return Ok(crate::supervisor::EXIT_USAGE);
    }
    let paths = Paths::from_env()?;
    if status && !paths.config_file().exists() {
        crate::ui::out("unknown");
        return Ok(EXIT_OK);
    }
    if !status {
        let code = prepare(&paths, agent).await?;
        if code != EXIT_OK {
            return Ok(code);
        }
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
                    "no active {} release. Fix: run 'zevbox update --agent {}'",
                    crate::agents::find(agent.id()).map_or(agent.id(), |a| a.display),
                    agent.id()
                ));
                return Ok(EXIT_PRECONDITION);
            }
        }
    };

    if status {
        if is_authenticated(&paths, agent.id(), &lock.auth_marker) {
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
    if agent.id() == "omlx" {
        return super::auth_omlx::run(&paths, &config, &lock, omlx).await;
    }

    match ensure_claude_default_model(&paths, &host_claude_settings()) {
        Ok(Some(model)) => crate::ui::out(&format!(
            "claude: default model set to {model} (state/claude/.claude/settings.json)"
        )),
        Ok(None) => {}
        Err(e) => crate::ui::err(&format!("cannot write the default Claude model: {e}")),
    }

    // I1: the auth container needs no project; it gets an empty throwaway dir
    // (removed on drop) instead of an allowed project or the cwd.
    let tmp_root = paths.state_root.join("tmp");
    crate::fsutil::ensure_private_dir(&tmp_root)?;
    let tmp = tempfile::Builder::new()
        .prefix("auth-")
        .tempdir_in(&tmp_root)?;
    let tmp_path = std::fs::canonicalize(tmp.path())?;
    let spec = auth_run_spec(&config, &paths, &lock, &tmp_path);

    // The login runs inside the VM, which cannot open the Mac's browser (I3).
    crate::ui::out(
        "claude: the browser will NOT open by itself (the login runs inside the VM).\n\
         1. Copy the https://claude.com/... URL printed below into your browser.\n\
         2. Sign in and authorize.\n\
         3. Paste the code shown by the browser at the 'Paste code here' prompt.",
    );

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

/// Quick start (DECISIONS R2): makes `auth` work from a blank install.
/// `init` if there is no config, starts the container runtime if stopped, and
/// builds the agent's release when it is missing or its image is gone.
/// Idempotent: everything in place → nothing done.
async fn prepare(paths: &Paths, agent: super::AgentArg) -> anyhow::Result<u8> {
    if !paths.config_file().exists() {
        let code = super::init::run().await?;
        if code != EXIT_OK {
            return Ok(code);
        }
    }
    let config = crate::config::load(&paths.config_file())?;
    let cli = crate::container::ContainerCli::new(config.container_binary.clone());
    let mut running = cli
        .system_status()
        .await
        .map(|s| s.running)
        .unwrap_or(false);
    if !running {
        let _ = cli.system_start().await;
        running = cli
            .system_status()
            .await
            .map(|s| s.running)
            .unwrap_or(false);
    }
    if !running {
        crate::ui::err("Apple Container system is not running. Fix: run 'container system start'");
        return Ok(EXIT_PRECONDITION);
    }

    let display = crate::agents::find(agent.id()).map_or(agent.id(), |a| a.display);
    let image_ready = match crate::release::active_lock(paths, agent.id()) {
        Ok(lock) => cli
            .image_list()
            .await
            .map(|images| {
                images
                    .iter()
                    .any(|i| crate::container::image_matches(&i.reference, &lock.image))
            })
            .unwrap_or(false),
        Err(_) => false,
    };
    if !image_ready {
        crate::ui::out(&format!(
            "{display}: no release yet, building it now (network, 1-3 min)…"
        ));
        let code = super::update::run_update(false, Some(agent)).await?;
        if code != EXIT_OK {
            return Ok(code);
        }
    }
    Ok(EXIT_OK)
}

/// Run spec of the Claude login container: only `tmp` (an empty directory) and
/// `state/claude` are mounted.
pub fn auth_run_spec(
    config: &crate::config::Config,
    paths: &Paths,
    lock: &crate::release::ReleaseLock,
    tmp: &std::path::Path,
) -> crate::container::RunSpec {
    let id8 = crate::container::new_instance_id();
    crate::container::RunSpec {
        name: format!("zevbox-claude-sh-{id8}"),
        instance_id: format!("auth-{id8}"),
        agent: "claude".to_string(),
        cpus: config.resources.cpus,
        memory: config.resources.memory.clone(),
        uid: config.host_uid,
        gid: config.host_gid,
        project: tmp.to_path_buf(),
        state_dir: paths.state_dir("claude"),
        image: lock.image.clone(),
        command: lock.auth_command.clone(),
        interactive: true,
    }
}

/// The host's own Claude settings, read-only (never mounted).
fn host_claude_settings() -> std::path::PathBuf {
    crate::paths::home_dir()
        .map(|home| home.join(".claude").join("settings.json"))
        .unwrap_or_default()
}

/// Container-side `~/.claude/settings.json` of the Claude agent.
fn claude_settings_path(paths: &Paths) -> std::path::PathBuf {
    paths
        .state_dir("claude")
        .join(".claude")
        .join("settings.json")
}

/// Without a default model the container's Claude starts on Haiku 4.5, which
/// has no Auto mode (Zed then falls back to Accept edits, DECISIONS R8). Writes
/// `{"model": "<m>"}` (0600) when the file is missing, where `<m>` is the
/// `model` of `host_settings` if it holds one, else "fable". Only `model` is
/// ever copied. Returns the model written, `None` if the file already exists.
pub fn ensure_claude_default_model(
    paths: &Paths,
    host_settings: &std::path::Path,
) -> std::io::Result<Option<String>> {
    let target = claude_settings_path(paths);
    if target.exists() {
        return Ok(None);
    }
    let model = std::fs::read_to_string(host_settings)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| v.get("model").and_then(|m| m.as_str()).map(str::to_string))
        .unwrap_or_else(|| "fable".to_string());
    if let Some(dir) = target.parent() {
        crate::fsutil::ensure_private_dir(dir)?;
    }
    let body = format!("{}\n", serde_json::json!({ "model": model }));
    crate::fsutil::write_atomic(&target, body.as_bytes())?;
    Ok(Some(model))
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

    // Absolute path: Zed-less shells may have a minimal PATH. The URL printed
    // above stays the fallback.
    match std::process::Command::new("/usr/bin/open")
        .arg(&auth_url)
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => crate::ui::err(&format!(
            "could not open the browser ({status}): open the URL above manually"
        )),
        Err(e) => crate::ui::err(&format!(
            "could not open the browser ({e}): open the URL above manually"
        )),
    }

    listener.set_nonblocking(true)?;
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(300);

    use std::io::Write;
    // Browsers also ask for /favicon.ico and the like: answer 404 and keep
    // waiting for the real callback (`GET /?…`).
    let (mut stream, req) = loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false)?;
                stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
                let req = read_request_head(&mut stream);
                if is_callback_request(&req) {
                    break (stream, req);
                }
                let _ = stream.write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
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

    let code = match callback_is_valid(&req, &state) {
        Ok(c) => c,
        Err(reason) => {
            let body: &[u8] = if reason == STATE_MISMATCH {
                b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\nConnection: close\r\n\r\nInvalid state"
            } else {
                b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n<h1>Authentication Failed</h1>"
            };
            let _ = stream.write_all(body);
            crate::ui::err(&format!("Google authentication failed: {reason}"));
            return Ok(EXIT_ERROR);
        }
    };

    let response = b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<!DOCTYPE html><html><body style=\"font-family: sans-serif; text-align: center; padding-top: 50px;\"><h2>Google authentication successful!</h2><p>You can close this tab and return to your terminal.</p></body></html>";
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

    let mut acp_token_json = serde_json::json!({
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
    // Also keep the access token and its expiry when Google sent them.
    if let (Some(obj), Some(access)) = (
        acp_token_json.as_object_mut(),
        token_resp.get("access_token").and_then(|v| v.as_str()),
    ) {
        obj.insert("token".into(), access.into());
        if let Some(secs) = token_resp.get("expires_in").and_then(|v| v.as_i64()) {
            let expiry = chrono::Utc::now() + chrono::Duration::seconds(secs);
            obj.insert(
                "expiry".into(),
                expiry
                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
                    .into(),
            );
        }
    }
    crate::fsutil::write_atomic(
        &acp_dir.join("acp_token.json"),
        serde_json::to_string_pretty(&acp_token_json)?.as_bytes(),
    )?;

    crate::ui::out("Google Antigravity authenticated successfully.");
    Ok(EXIT_OK)
}

const STATE_MISMATCH: &str = "state mismatch";

/// True for `GET /?…`, the only request that can carry the OAuth redirect.
fn is_callback_request(req: &str) -> bool {
    let mut parts = req.lines().next().unwrap_or("").split_whitespace();
    parts.next() == Some("GET") && parts.next().is_some_and(|p| p.starts_with("/?"))
}

/// The authorization code of the callback request, if its `state` is the one
/// we generated. `Err` carries the reason shown to the user.
fn callback_is_valid(req: &str, expected_state: &str) -> Result<String, String> {
    if extract_query_param(req, "state").as_deref() != Some(expected_state) {
        return Err(STATE_MISMATCH.to_string());
    }
    extract_query_param(req, "code").ok_or_else(|| {
        extract_query_param(req, "error").unwrap_or_else(|| "missing code parameter".to_string())
    })
}

/// Reads the HTTP request head from the browser's connection: loops until the first
/// line is complete (a browser may write it in several segments or pre-connect first),
/// capped at 4096 bytes; the 5 s read timeout set on the stream bounds the wait.
fn read_request_head(stream: &mut std::net::TcpStream) -> String {
    use std::io::Read;
    let mut buf = [0u8; 4096];
    let mut len = 0;
    while len < buf.len() {
        match stream.read(&mut buf[len..]) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                len += n;
                if buf[..len].contains(&b'\n') {
                    break;
                }
            }
        }
    }
    String::from_utf8_lossy(&buf[..len]).into_owned()
}

/// The one definition of « authenticated », shared by `auth --status`, `setup` and
/// `doctor`. Claude: the credentials file exists. Antigravity: `acp_token.json`
/// holds a non-empty `refresh_token` (DECISIONS R5). oMLX: the settings name a
/// server and a key.
pub fn is_authenticated(paths: &Paths, agent: &str, auth_marker: &str) -> bool {
    let marker = paths.state_dir(agent).join(auth_marker);
    match agent {
        "antigravity" => has_refresh_token(&marker),
        "omlx" => crate::omlx::is_configured(&marker),
        _ => marker.exists(),
    }
}

/// `acp_token.json` counts as authenticated only with a non-empty `refresh_token`.
fn has_refresh_token(path: &std::path::Path) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| {
            v.get("refresh_token")
                .and_then(|t| t.as_str())
                .map(|t| !t.is_empty())
        })
        .unwrap_or(false)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_url_vectors() {
        for (input, want) in [
            ("", ""),
            ("f", "Zg"),
            ("fo", "Zm8"),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg"),
        ] {
            assert_eq!(base64_url_encode(input.as_bytes()), want);
        }
    }

    #[test]
    fn url_encode_reserved() {
        assert_eq!(url_encode("a b/c"), "a%20b%2Fc");
    }

    #[test]
    fn query_params_are_decoded() {
        let req = "GET /?code=x%2By&state=s HTTP/1.1\r\n";
        assert_eq!(extract_query_param(req, "code").as_deref(), Some("x+y"));
        assert_eq!(extract_query_param(req, "state").as_deref(), Some("s"));
        assert_eq!(extract_query_param(req, "other"), None);
        assert_eq!(percent_decode("a%20b%2Fc"), "a b/c");
    }

    #[test]
    fn callback_state_is_checked() {
        let ok = "GET /?code=x%2By&state=s HTTP/1.1\r\n";
        assert_eq!(callback_is_valid(ok, "s"), Ok("x+y".to_string()));
        assert_eq!(
            callback_is_valid(ok, "other"),
            Err(STATE_MISMATCH.to_string())
        );
        let no_state = "GET /?code=c HTTP/1.1\r\n";
        assert_eq!(
            callback_is_valid(no_state, "s"),
            Err(STATE_MISMATCH.to_string())
        );
        let denied = "GET /?error=access_denied&state=s HTTP/1.1\r\n";
        assert_eq!(
            callback_is_valid(denied, "s"),
            Err("access_denied".to_string())
        );
    }

    #[test]
    fn only_get_root_query_is_a_callback() {
        assert!(is_callback_request("GET /?code=c&state=s HTTP/1.1\r\n"));
    }

    #[test]
    fn request_head_survives_a_fragmented_write() {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let writer = std::thread::spawn(move || {
            let mut c = std::net::TcpStream::connect(addr).unwrap();
            c.write_all(b"GET /?code=abc&sta").unwrap();
            c.flush().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(150));
            c.write_all(b"te=xyz HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        });
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let head = read_request_head(&mut stream);
        writer.join().unwrap();
        assert!(
            head.starts_with("GET /?code=abc&state=xyz HTTP/1.1\r\n"),
            "{head}"
        );
        assert!(is_callback_request(&head));
        assert!(!is_callback_request("GET /favicon.ico HTTP/1.1\r\n"));
        assert!(!is_callback_request("POST /?code=c HTTP/1.1\r\n"));
        assert!(!is_callback_request(""));
    }
}

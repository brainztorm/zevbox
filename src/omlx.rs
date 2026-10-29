//! oMLX agent (DECISIONS 2026-10-29): the Claude adapter, in its own VM and state
//! folder, pointed at the user's oMLX server (Anthropic-compatible `/v1/messages`).
//!
//! Everything lives in `state/omlx/.claude/settings.json`, read by Claude Code in
//! the VM: server URL and key (`env`), model list (`availableModels`) and a
//! `PostModelSwitch` hook that asks the server to unload the model left behind.
//! The key never appears on a command line, in `-e`, or in a log (I9).

use serde_json::{json, Map, Value};
use std::path::Path;

/// Relative to `state/omlx`: the settings file is the authentication marker.
pub const AUTH_MARKER: &str = ".claude/settings.json";

/// One model offered by the server; `size` (bytes) when the server reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerModel {
    pub id: String,
    pub size: Option<u64>,
}

/// Runs in the VM (`/bin/sh`, Claude Code hook) after a model switch: unloads the
/// previous model, except the helper model Claude Code keeps using for small
/// background requests (unloading it would reload it at once). Reads the hook
/// input on stdin; the URL and key come from the settings `env`. Never fails the
/// switch (`exit 0`).
pub const UNLOAD_HOOK: &str = r#"i=$(cat); from=$(printf %s "$i" | jq -r '.from_model // empty'); to=$(printf %s "$i" | jq -r '.to_model // empty'); if [ -n "$from" ] && [ "$from" != "$to" ] && [ "$from" != "$ZEVBOX_OMLX_HELPER_MODEL" ]; then curl --silent --output /dev/null --max-time 30 -X POST -H "Authorization: Bearer $ANTHROPIC_AUTH_TOKEN" "$ANTHROPIC_BASE_URL/v1/models/$(printf %s "$from" | jq -sRr @uri)/unload"; fi; exit 0"#;

/// `http(s)://host[:port]` without path, query or credentials; trailing `/`
/// removed. `localhost`/`127.*` are refused: inside the VM they are the VM itself.
pub fn validate_url(url: &str) -> Result<String, String> {
    let url = url.trim().trim_end_matches('/');
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .ok_or_else(|| format!("'{url}' must start with http:// or https://"))?;
    let valid_char = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':');
    if rest.is_empty() || !rest.chars().all(valid_char) {
        return Err(format!(
            "'{url}' must be http://<host>[:<port>] with no path, e.g. http://192.168.1.8:8000"
        ));
    }
    let host = rest.split(':').next().unwrap_or("");
    if host == "localhost" || host.starts_with("127.") {
        return Err(format!(
            "'{url}' points at the VM itself; use the server's network address, e.g. http://192.168.1.8:8000"
        ));
    }
    Ok(url.to_string())
}

/// Keys go into a curl config read from stdin and into settings.json: a plain
/// token alphabet keeps both free of quoting problems.
pub fn validate_key(key: &str) -> Result<String, String> {
    let key = key.trim();
    let ok = !key.is_empty()
        && key.len() <= 512
        && key.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~' | '+' | '/' | '=')
        });
    if ok {
        Ok(key.to_string())
    } else {
        Err("the API key must be one line of letters, digits and -_.~+/=".to_string())
    }
}

/// Model ids end up in settings.json and in the unload URL (encoded by the hook).
fn valid_model_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 200
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':' | '/' | '@'))
}

/// Models from oMLX's `/v1/models/status` (`models[].id`, `estimated_size`) or,
/// for any other server, the OpenAI-style `/v1/models` (`data[].id`, no size).
/// Ids with unexpected characters are skipped.
pub fn parse_models(text: &str) -> Result<Vec<ServerModel>, String> {
    let root: Value =
        serde_json::from_str(text).map_err(|e| format!("the server sent invalid JSON: {e}"))?;
    let list = root
        .get("models")
        .or_else(|| root.get("data"))
        .and_then(Value::as_array)
        .ok_or_else(|| "the server's model list has no 'models' or 'data' array".to_string())?;
    let models: Vec<ServerModel> = list
        .iter()
        .filter_map(|m| {
            let id = m.get("id")?.as_str()?;
            valid_model_id(id).then(|| ServerModel {
                id: id.to_string(),
                size: m.get("estimated_size").and_then(Value::as_u64),
            })
        })
        .collect();
    if models.is_empty() {
        return Err("the server offers no model".to_string());
    }
    Ok(models)
}

/// The smallest model by reported size; without sizes, the first one listed.
pub fn smallest(models: &[ServerModel]) -> Option<&ServerModel> {
    models
        .iter()
        .filter(|m| m.size.is_some())
        .min_by_key(|m| m.size)
        .or_else(|| models.first())
}

/// The settings.json Claude Code reads in the VM. `existing` is the current file
/// (keys zevbox does not manage are kept, e.g. what Claude Code writes itself).
/// The helper model serves Claude Code's small background requests (haiku slot).
pub fn settings_json(
    existing: Option<Value>,
    url: &str,
    key: &str,
    default_model: &str,
    helper_model: &str,
    models: &[ServerModel],
) -> Value {
    let mut root = match existing {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    };
    let mut env = match root.remove("env") {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    };
    for (name, value) in [
        ("ANTHROPIC_BASE_URL", url),
        ("ANTHROPIC_AUTH_TOKEN", key),
        ("ANTHROPIC_MODEL", default_model),
        ("ANTHROPIC_DEFAULT_OPUS_MODEL", default_model),
        ("ANTHROPIC_DEFAULT_SONNET_MODEL", default_model),
        ("ANTHROPIC_DEFAULT_HAIKU_MODEL", helper_model),
        ("ANTHROPIC_SMALL_FAST_MODEL", helper_model),
        ("ZEVBOX_OMLX_HELPER_MODEL", helper_model),
        // No telemetry, error reporting or update checks towards Anthropic.
        ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1"),
    ] {
        env.insert(name.to_string(), Value::String(value.to_string()));
    }
    root.insert("env".to_string(), Value::Object(env));
    root.insert("model".to_string(), json!(default_model));
    root.insert(
        "availableModels".to_string(),
        json!(models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>()),
    );
    let mut hooks = match root.remove("hooks") {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    };
    hooks.insert(
        "PostModelSwitch".to_string(),
        json!([{ "hooks": [{ "type": "command", "command": UNLOAD_HOOK }] }]),
    );
    root.insert("hooks".to_string(), Value::Object(hooks));
    Value::Object(root)
}

/// Authenticated = the settings file names a server and a key (shared by
/// `auth --status`, `setup`, `doctor`).
pub fn is_configured(settings: &Path) -> bool {
    configured_url(settings).is_some()
}

/// The server URL of a configured settings file (with a key), for messages.
pub fn configured_url(settings: &Path) -> Option<String> {
    let text = std::fs::read_to_string(settings).ok()?;
    let root: Value = serde_json::from_str(&text).ok()?;
    let env = root.get("env")?;
    let non_empty = |name: &str| {
        env.get(name)
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };
    non_empty("ANTHROPIC_AUTH_TOKEN")?;
    non_empty("ANTHROPIC_BASE_URL")
}

/// curl config read on stdin: keeps the key out of the process list.
pub fn curl_auth_config(key: &str) -> String {
    format!("header = \"Authorization: Bearer {key}\"\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUS: &str = r#"{"models":[
        {"id":"Big-Model","estimated_size":453635360875,"loaded":false},
        {"id":"Qwen3.8-27B-4bit","estimated_size":16857268416,"loaded":true},
        {"id":"bad id; rm -rf","estimated_size":1}]}"#;

    #[test]
    fn urls_are_checked() {
        assert_eq!(
            validate_url(" http://192.168.1.8:8000/ ").as_deref(),
            Ok("http://192.168.1.8:8000")
        );
        assert!(validate_url("https://studio.local").is_ok());
        for bad in [
            "192.168.1.8:8000",
            "ftp://x",
            "http://",
            "http://h/v1",
            "http://u:p@h",
            "http://h?x",
            "http://localhost:8000",
            "http://127.0.0.1:8000",
            "http://h\" -o /x",
        ] {
            assert!(validate_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn keys_are_checked() {
        assert_eq!(validate_key("omlx-abc123\n").as_deref(), Ok("omlx-abc123"));
        for bad in ["", "a b", "a\"b", "a\\b", "a\nb"] {
            assert!(validate_key(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn models_from_status_and_openai_lists() {
        let models = parse_models(STATUS).unwrap();
        assert_eq!(models.len(), 2, "invalid ids are skipped");
        assert_eq!(smallest(&models).unwrap().id, "Qwen3.8-27B-4bit");
        let openai = parse_models(r#"{"object":"list","data":[{"id":"a"},{"id":"b"}]}"#).unwrap();
        assert_eq!(smallest(&openai).unwrap().id, "a", "no size: first listed");
        assert!(parse_models(r#"{"data":[]}"#).is_err());
        assert!(parse_models("not json").is_err());
    }

    #[test]
    fn settings_hold_server_models_and_hook_and_keep_other_keys() {
        let models = parse_models(STATUS).unwrap();
        let existing = json!({"theme": "dark", "env": {"FOO": "1"}, "model": "old"});
        let s = settings_json(
            Some(existing),
            "http://192.168.1.8:8000",
            "k-1",
            "Big-Model",
            "Qwen3.8-27B-4bit",
            &models,
        );
        assert_eq!(s["theme"], "dark");
        assert_eq!(s["env"]["FOO"], "1");
        assert_eq!(s["model"], "Big-Model");
        assert_eq!(s["env"]["ANTHROPIC_BASE_URL"], "http://192.168.1.8:8000");
        assert_eq!(s["env"]["ANTHROPIC_AUTH_TOKEN"], "k-1");
        assert_eq!(s["env"]["ANTHROPIC_SMALL_FAST_MODEL"], "Qwen3.8-27B-4bit");
        assert_eq!(s["env"]["ZEVBOX_OMLX_HELPER_MODEL"], "Qwen3.8-27B-4bit");
        assert_eq!(
            s["availableModels"],
            json!(["Big-Model", "Qwen3.8-27B-4bit"])
        );
        assert_eq!(
            s["hooks"]["PostModelSwitch"][0]["hooks"][0]["command"],
            UNLOAD_HOOK
        );
        // F6: the key goes in the bearer-token variable only; nothing else is set.
        let names: Vec<&String> = s["env"].as_object().unwrap().keys().collect();
        assert_eq!(names.len(), 10, "{names:?}");
    }

    #[test]
    fn configured_needs_url_and_key() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("settings.json");
        assert!(!is_configured(&f));
        std::fs::write(&f, r#"{"model":"x"}"#).unwrap();
        assert!(
            !is_configured(&f),
            "a bare model file is not a server config"
        );
        std::fs::write(
            &f,
            r#"{"env":{"ANTHROPIC_BASE_URL":"http://h:1","ANTHROPIC_AUTH_TOKEN":""}}"#,
        )
        .unwrap();
        assert!(!is_configured(&f));
        std::fs::write(
            &f,
            r#"{"env":{"ANTHROPIC_BASE_URL":"http://h:1","ANTHROPIC_AUTH_TOKEN":"k"}}"#,
        )
        .unwrap();
        assert_eq!(configured_url(&f).as_deref(), Some("http://h:1"));
    }

    #[test]
    fn unload_hook_runs_in_the_vm_and_never_fails() {
        assert!(UNLOAD_HOOK.ends_with("exit 0"));
        assert!(UNLOAD_HOOK.contains("$ZEVBOX_OMLX_HELPER_MODEL"));
        assert!(UNLOAD_HOOK.contains("/unload"));
    }
}

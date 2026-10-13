//! §9.1 — Pure policy tests. PROTECTED FILE: do not edit (see tests/PROTECTED.sha256).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{json, Value};
use zevbox::relay::jsonrpc::{INVALID_PARAMS, INVALID_REQUEST, METHOD_NOT_FOUND, SERVER_ERROR};
use zevbox::relay::policy::{
    additional_directories_message, auth_hint, blocked_message, cwd_message, decide, Direction,
    Policy, RelayState, Verdict,
};

const ROOT: &str = "/Users/test/code/proj";

fn policy() -> Policy {
    Policy::new(ROOT, "claude")
}

fn c2a(state: &mut RelayState, msg: Value) -> Verdict {
    decide(&policy(), state, Direction::ClientToAgent, msg)
}

fn a2c(state: &mut RelayState, msg: Value) -> Verdict {
    decide(&policy(), state, Direction::AgentToClient, msg)
}

fn req(id: Value, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

fn notif(method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "method": method, "params": params})
}

fn ok_resp(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

/// Asserts a ReplyToSender error with this id and code; returns the message.
fn expect_error(v: Verdict, id: Value, code: i64) -> String {
    match v {
        Verdict::ReplyToSender(m) => {
            assert_eq!(m["jsonrpc"], "2.0", "reply must be JSON-RPC 2.0: {m}");
            assert_eq!(m["id"], id, "reply must reuse the request id: {m}");
            assert_eq!(m["error"]["code"], code, "wrong error code: {m}");
            assert!(m.get("result").is_none());
            m["error"]["message"].as_str().unwrap().to_string()
        }
        other => panic!("expected ReplyToSender error {code}, got {other:?}"),
    }
}

fn expect_forward(v: Verdict) -> Value {
    match v {
        Verdict::Forward(m) => m,
        other => panic!("expected Forward, got {other:?}"),
    }
}

fn expect_drop(v: Verdict) {
    assert!(
        matches!(v, Verdict::Drop { .. }),
        "expected Drop, got {v:?}"
    );
}

fn session_params(cwd: &str) -> Value {
    json!({
        "cwd": cwd,
        "mcpServers": [{"name": "personal", "command": "/usr/local/bin/x", "args": [], "env": []}],
        "_meta": {"claudeCode": {"options": {"permissionMode": "bypassPermissions"}}}
    })
}

// ---------- agent → client: host access is never delegated (I3) ----------

#[test]
fn blocks_fs_read() {
    let mut s = RelayState::default();
    let msg = expect_error(
        a2c(
            &mut s,
            req(
                json!(7),
                "fs/read_text_file",
                json!({"sessionId": "s", "path": "/etc/passwd"}),
            ),
        ),
        json!(7),
        METHOD_NOT_FOUND,
    );
    assert_eq!(msg, blocked_message("fs/read_text_file"));
}

#[test]
fn blocks_fs_write() {
    let mut s = RelayState::default();
    expect_error(
        a2c(
            &mut s,
            req(
                json!("w1"),
                "fs/write_text_file",
                json!({"sessionId": "s", "path": "/tmp/x", "content": "x"}),
            ),
        ),
        json!("w1"),
        METHOD_NOT_FOUND,
    );
}

#[test]
fn blocks_all_terminal_methods() {
    let mut s = RelayState::default();
    for (i, m) in [
        "terminal/create",
        "terminal/output",
        "terminal/wait_for_exit",
        "terminal/kill",
        "terminal/release",
    ]
    .iter()
    .enumerate()
    {
        let msg = expect_error(
            a2c(
                &mut s,
                req(json!(i), m, json!({"sessionId": "s", "command": "rm"})),
            ),
            json!(i),
            METHOD_NOT_FOUND,
        );
        assert_eq!(msg, blocked_message(m));
    }
}

#[test]
fn blocks_elicitation() {
    let mut s = RelayState::default();
    expect_error(
        a2c(
            &mut s,
            req(
                json!(1),
                "elicitation/create",
                json!({"url": "https://evil.example"}),
            ),
        ),
        json!(1),
        METHOD_NOT_FOUND,
    );
    expect_drop(a2c(&mut s, notif("elicitation/complete", json!({}))));
}

#[test]
fn blocks_unknown_agent_request() {
    let mut s = RelayState::default();
    expect_error(
        a2c(&mut s, req(json!(1), "host/exec", json!({}))),
        json!(1),
        METHOD_NOT_FOUND,
    );
}

#[test]
fn blocks_unknown_client_request() {
    let mut s = RelayState::default();
    expect_error(
        c2a(&mut s, req(json!(1), "session/do_something_new", json!({}))),
        json!(1),
        METHOD_NOT_FOUND,
    );
}

#[test]
fn blocks_underscore_extension() {
    let mut s = RelayState::default();
    expect_error(
        a2c(&mut s, req(json!(1), "_zed.dev/apply_patch", json!({}))),
        json!(1),
        METHOD_NOT_FOUND,
    );
    expect_error(
        c2a(&mut s, req(json!(2), "_claude/setting", json!({}))),
        json!(2),
        METHOD_NOT_FOUND,
    );
}

#[test]
fn drops_unknown_notifications_both_ways() {
    let mut s = RelayState::default();
    expect_drop(a2c(&mut s, notif("terminal/created", json!({}))));
    expect_drop(a2c(&mut s, notif("_custom/thing", json!({}))));
    expect_drop(c2a(&mut s, notif("session/unknown", json!({}))));
}

// ---------- initialize: capabilities and auth methods (F1, F2) ----------

fn zed_initialize(id: Value) -> Value {
    req(
        id,
        "initialize",
        json!({
            "protocolVersion": 1,
            "clientCapabilities": {
                "fs": {"readTextFile": true, "writeTextFile": true},
                "terminal": true,
                "auth": {"terminal": true, "_meta": {"gateway": true}},
                "elicitation": {"url": {}},
                "session": {"configOptions": true},
                "_meta": {"terminal-auth": true, "terminal_output": true}
            },
            "clientInfo": {"name": "zed", "version": "1.0.0"}
        }),
    )
}

#[test]
fn rewrites_client_capabilities() {
    let mut s = RelayState::default();
    let out = expect_forward(c2a(&mut s, zed_initialize(json!(0))));
    assert_eq!(out["method"], "initialize");
    assert_eq!(out["id"], 0);
    assert_eq!(out["params"]["protocolVersion"], 1);
    assert_eq!(
        out["params"]["clientCapabilities"],
        json!({
            "fs": {"readTextFile": false, "writeTextFile": false},
            "terminal": false,
            "auth": {"terminal": false},
            "session": {"configOptions": true}
        })
    );
}

#[test]
fn rewrites_capabilities_without_session_field() {
    let mut s = RelayState::default();
    let out = expect_forward(c2a(
        &mut s,
        req(
            json!(0),
            "initialize",
            json!({"protocolVersion": 1, "clientCapabilities": {"terminal": true}}),
        ),
    ));
    assert_eq!(
        out["params"]["clientCapabilities"],
        json!({"fs": {"readTextFile": false, "writeTextFile": false}, "terminal": false, "auth": {"terminal": false}})
    );
}

/// Shape of the real claude-agent-acp 0.89.x `initialize` result.
fn claude_initialize_result() -> Value {
    json!({
        "protocolVersion": 1,
        "agentCapabilities": {"loadSession": true, "promptCapabilities": {"image": true}},
        "authMethods": [
            {
                "id": "claude-ai-login", "name": "Claude Subscription", "type": "terminal",
                "args": ["--cli", "auth", "login", "--claudeai"],
                "_meta": {"terminal-auth": {"command": "/usr/local/bin/node", "args": ["x", "--cli"], "label": "Claude Login"}}
            },
            {
                "id": "console-login", "name": "Anthropic Console", "type": "terminal",
                "args": ["--cli", "auth", "login", "--console"]
            },
            {
                "id": "legacy", "name": "Legacy meta only",
                "_meta": {"terminal-auth": {"command": "/bin/sh", "args": ["-c", "id"]}}
            },
            {"id": "agent-login", "name": "Agent login", "type": "agent", "description": "handled by agent"}
        ]
    })
}

#[test]
fn strips_terminal_auth_methods() {
    let mut s = RelayState::default();
    expect_forward(c2a(&mut s, zed_initialize(json!(0))));
    let out = expect_forward(a2c(&mut s, ok_resp(json!(0), claude_initialize_result())));
    let methods = out["result"]["authMethods"].as_array().unwrap();
    let ids: Vec<&str> = methods.iter().map(|m| m["id"].as_str().unwrap()).collect();
    // "legacy" has no type (= agent) but its _meta must be removed.
    assert_eq!(ids, vec!["legacy", "agent-login"]);
    for m in methods {
        assert!(
            m.get("_meta").is_none(),
            "auth method _meta must be stripped: {m}"
        );
        assert_ne!(m["type"], "terminal");
    }
    assert_eq!(out["result"]["agentCapabilities"]["loadSession"], true);
}

#[test]
fn authenticate_accepts_kept_method() {
    let mut s = RelayState::default();
    expect_forward(c2a(&mut s, zed_initialize(json!(0))));
    expect_forward(a2c(&mut s, ok_resp(json!(0), claude_initialize_result())));
    let out = expect_forward(c2a(
        &mut s,
        req(json!(1), "authenticate", json!({"methodId": "agent-login"})),
    ));
    assert_eq!(out["params"]["methodId"], "agent-login");
}

#[test]
fn authenticate_rejects_stripped_method() {
    let mut s = RelayState::default();
    expect_forward(c2a(&mut s, zed_initialize(json!(0))));
    expect_forward(a2c(&mut s, ok_resp(json!(0), claude_initialize_result())));
    expect_error(
        c2a(
            &mut s,
            req(
                json!(1),
                "authenticate",
                json!({"methodId": "claude-ai-login"}),
            ),
        ),
        json!(1),
        INVALID_PARAMS,
    );
}

// ---------- sessions: cwd lock, MCP, _meta (I5, F3) ----------

#[test]
fn session_new_accepts_exact_root() {
    let mut s = RelayState::default();
    let out = expect_forward(c2a(
        &mut s,
        req(json!(3), "session/new", session_params(ROOT)),
    ));
    assert_eq!(out["params"]["cwd"], ROOT);
    assert_eq!(out["params"]["mcpServers"], json!([]));
    assert!(
        out["params"].get("_meta").is_none(),
        "_meta must be stripped (F3)"
    );
}

#[test]
fn session_new_accepts_trailing_slash() {
    let mut s = RelayState::default();
    expect_forward(c2a(
        &mut s,
        req(json!(3), "session/new", session_params(&format!("{ROOT}/"))),
    ));
}

#[test]
fn policy_new_normalizes_trailing_slash() {
    assert_eq!(Policy::new("/a/b/", "claude").project_root, "/a/b");
    assert_eq!(Policy::new("/a/b", "claude").project_root, "/a/b");
}

fn assert_cwd_rejected(cwd: &str) {
    let mut s = RelayState::default();
    let msg = expect_error(
        c2a(&mut s, req(json!(9), "session/new", session_params(cwd))),
        json!(9),
        INVALID_PARAMS,
    );
    assert_eq!(msg, cwd_message(ROOT, cwd), "cwd={cwd}");
}

#[test]
fn session_new_rejects_other_dir() {
    assert_cwd_rejected("/Users/test/code/other");
    assert_cwd_rejected("/Users/test");
    assert_cwd_rejected("/");
}

#[test]
fn session_new_rejects_subdirectory() {
    assert_cwd_rejected("/Users/test/code/proj/src");
}

#[test]
fn session_new_rejects_parent_traversal() {
    assert_cwd_rejected("/Users/test/code/proj/..");
    assert_cwd_rejected("/Users/test/code/proj/../proj");
}

#[test]
fn session_new_rejects_prefix_trick() {
    assert_cwd_rejected("/Users/test/code/proj-evil");
    assert_cwd_rejected("/Users/test/code/projx");
}

#[test]
fn session_new_rejects_relative() {
    assert_cwd_rejected("proj");
    assert_cwd_rejected("./Users/test/code/proj");
    assert_cwd_rejected("");
}

#[test]
fn session_new_rejects_double_slash() {
    assert_cwd_rejected("/Users/test//code/proj");
}

#[test]
fn session_new_rejects_dot_component() {
    assert_cwd_rejected("/Users/test/code/./proj");
}

#[test]
fn session_new_rejects_nul_byte() {
    assert_cwd_rejected("/Users/test/code/proj\u{0}");
}

#[test]
fn session_new_rejects_missing_or_non_string_cwd() {
    let mut s = RelayState::default();
    expect_error(
        c2a(
            &mut s,
            req(json!(1), "session/new", json!({"mcpServers": []})),
        ),
        json!(1),
        INVALID_PARAMS,
    );
    expect_error(
        c2a(
            &mut s,
            req(
                json!(2),
                "session/new",
                json!({"cwd": 42, "mcpServers": []}),
            ),
        ),
        json!(2),
        INVALID_PARAMS,
    );
}

#[test]
fn session_new_rejects_additional_directories() {
    let mut s = RelayState::default();
    let mut p = session_params(ROOT);
    p["additionalDirectories"] = json!(["/Users/test/secrets"]);
    let msg = expect_error(
        c2a(&mut s, req(json!(4), "session/new", p)),
        json!(4),
        INVALID_PARAMS,
    );
    assert_eq!(msg, additional_directories_message());
}

#[test]
fn session_new_accepts_empty_additional_directories() {
    let mut s = RelayState::default();
    let mut p = session_params(ROOT);
    p["additionalDirectories"] = json!([]);
    expect_forward(c2a(&mut s, req(json!(4), "session/new", p)));
}

#[test]
fn session_load_resume_fork_apply_same_rules() {
    for method in ["session/load", "session/resume", "session/fork"] {
        let mut s = RelayState::default();
        let mut ok = session_params(ROOT);
        ok["sessionId"] = json!("abc");
        let out = expect_forward(c2a(&mut s, req(json!(1), method, ok)));
        assert_eq!(out["params"]["mcpServers"], json!([]), "{method}");
        assert!(out["params"].get("_meta").is_none(), "{method}");
        assert_eq!(out["params"]["sessionId"], "abc", "{method}");

        let mut bad = session_params("/elsewhere");
        bad["sessionId"] = json!("abc");
        expect_error(
            c2a(&mut s, req(json!(2), method, bad)),
            json!(2),
            INVALID_PARAMS,
        );
    }
}

#[test]
fn mcp_servers_are_emptied_or_added() {
    let mut s = RelayState::default();
    let out = expect_forward(c2a(
        &mut s,
        req(json!(1), "session/new", json!({"cwd": ROOT})),
    ));
    assert_eq!(out["params"]["mcpServers"], json!([]));
}

// ---------- response routing by id (I4) ----------

#[test]
fn response_with_unknown_id_is_dropped() {
    let mut s = RelayState::default();
    expect_drop(a2c(&mut s, ok_resp(json!(99), json!({"forged": true}))));
    expect_drop(c2a(&mut s, ok_resp(json!(98), json!({"outcome": "allow"}))));
}

#[test]
fn response_forwarded_once() {
    let mut s = RelayState::default();
    expect_forward(c2a(
        &mut s,
        req(
            json!(5),
            "session/prompt",
            json!({"sessionId": "s", "prompt": []}),
        ),
    ));
    expect_forward(a2c(
        &mut s,
        ok_resp(json!(5), json!({"stopReason": "end_turn"})),
    ));
    expect_drop(a2c(
        &mut s,
        ok_resp(json!(5), json!({"stopReason": "end_turn"})),
    ));
}

#[test]
fn error_response_is_routed_like_result() {
    let mut s = RelayState::default();
    expect_forward(c2a(
        &mut s,
        req(
            json!(5),
            "session/prompt",
            json!({"sessionId": "s", "prompt": []}),
        ),
    ));
    expect_forward(a2c(
        &mut s,
        json!({"jsonrpc": "2.0", "id": 5, "error": {"code": -32603, "message": "boom"}}),
    ));
}

#[test]
fn agent_request_response_round_trip() {
    let mut s = RelayState::default();
    expect_forward(a2c(
        &mut s,
        req(
            json!("p1"),
            "session/request_permission",
            json!({"sessionId": "s", "options": []}),
        ),
    ));
    expect_forward(c2a(
        &mut s,
        ok_resp(json!("p1"), json!({"outcome": {"outcome": "cancelled"}})),
    ));
    expect_drop(c2a(&mut s, ok_resp(json!("p1"), json!({}))));
}

#[test]
fn response_to_blocked_request_is_never_expected() {
    let mut s = RelayState::default();
    expect_error(
        a2c(
            &mut s,
            req(json!(1), "fs/read_text_file", json!({"path": "/x"})),
        ),
        json!(1),
        METHOD_NOT_FOUND,
    );
    // Zed never saw request 1, so a "response" from Zed with that id is forged.
    expect_drop(c2a(&mut s, ok_resp(json!(1), json!({"content": "secret"}))));
}

#[test]
fn duplicate_pending_id_rejected() {
    let mut s = RelayState::default();
    expect_forward(c2a(
        &mut s,
        req(
            json!(1),
            "session/prompt",
            json!({"sessionId": "s", "prompt": []}),
        ),
    ));
    expect_error(
        c2a(
            &mut s,
            req(
                json!(1),
                "session/prompt",
                json!({"sessionId": "s", "prompt": []}),
            ),
        ),
        json!(1),
        INVALID_REQUEST,
    );
}

#[test]
fn string_and_number_ids_distinct() {
    let mut s = RelayState::default();
    expect_forward(c2a(
        &mut s,
        req(
            json!(1),
            "session/prompt",
            json!({"sessionId": "s", "prompt": []}),
        ),
    ));
    expect_forward(c2a(
        &mut s,
        req(
            json!("1"),
            "session/prompt",
            json!({"sessionId": "s", "prompt": []}),
        ),
    ));
    expect_forward(a2c(&mut s, ok_resp(json!("1"), json!({}))));
    expect_forward(a2c(&mut s, ok_resp(json!(1), json!({}))));
    expect_drop(a2c(&mut s, ok_resp(json!(1), json!({}))));
}

// ---------- allowed traffic passes unchanged ----------

#[test]
fn permission_request_forwarded() {
    let mut s = RelayState::default();
    let m = req(
        json!(1),
        "session/request_permission",
        json!({"sessionId": "s", "toolCall": {}, "options": []}),
    );
    assert_eq!(expect_forward(a2c(&mut s, m.clone())), m);
}

#[test]
fn session_update_forwarded() {
    let mut s = RelayState::default();
    let m = notif(
        "session/update",
        json!({"sessionId": "s", "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "hi"}}}),
    );
    assert_eq!(expect_forward(a2c(&mut s, m.clone())), m);
}

#[test]
fn prompt_forwarded_unchanged() {
    let mut s = RelayState::default();
    let m = req(
        json!(2),
        "session/prompt",
        json!({"sessionId": "s", "prompt": [{"type": "text", "text": "hello"}]}),
    );
    assert_eq!(expect_forward(c2a(&mut s, m.clone())), m);
}

#[test]
fn allowed_client_requests_forwarded() {
    let mut s = RelayState::default();
    for (i, m) in [
        "session/set_mode",
        "session/set_model",
        "session/set_config_option",
        "session/list",
        "session/close",
        "session/delete",
        "logout",
    ]
    .iter()
    .enumerate()
    {
        let msg = req(json!(100 + i), m, json!({"sessionId": "s"}));
        assert_eq!(expect_forward(c2a(&mut s, msg.clone())), msg, "{m}");
    }
}

#[test]
fn cancel_notifications_forwarded() {
    let mut s = RelayState::default();
    let c = notif("session/cancel", json!({"sessionId": "s"}));
    assert_eq!(expect_forward(c2a(&mut s, c.clone())), c);
    let r = notif("$/cancel_request", json!({"requestId": 3}));
    assert_eq!(expect_forward(c2a(&mut s, r.clone())), r);
    assert_eq!(expect_forward(a2c(&mut s, r.clone())), r);
}

#[test]
fn auth_required_error_gets_hint() {
    let mut s = RelayState::default();
    expect_forward(c2a(
        &mut s,
        req(json!(8), "session/new", session_params(ROOT)),
    ));
    let out = expect_forward(a2c(
        &mut s,
        json!({"jsonrpc": "2.0", "id": 8, "error": {"code": SERVER_ERROR, "message": "Authentication required"}}),
    ));
    assert_eq!(
        out["error"]["message"],
        format!("Authentication required{}", auth_hint("claude"))
    );
}

// ---------- protocol violations are fatal (§7.2) ----------

#[test]
fn invalid_messages_are_fatal() {
    let mut s = RelayState::default();
    for bad in [
        json!({"jsonrpc": "1.0", "id": 1, "method": "initialize"}),
        json!({"id": 1, "method": "initialize"}),
        json!({"jsonrpc": "2.0"}),
        json!({"jsonrpc": "2.0", "id": 1}),
        json!({"jsonrpc": "2.0", "id": 1, "method": 5}),
        json!({"jsonrpc": "2.0", "id": {"x": 1}, "method": "session/prompt"}),
        json!([{"jsonrpc": "2.0", "id": 1, "method": "initialize"}]),
        json!("text"),
    ] {
        let v = c2a(&mut s, bad.clone());
        assert!(matches!(v, Verdict::Fatal { .. }), "{bad} → {v:?}");
    }
}

// ---------- exact user-facing texts (§7.3) ----------

#[test]
fn messages_match_plan_exactly() {
    assert_eq!(
        blocked_message("fs/read_text_file"),
        "zevbox: 'fs/read_text_file' is blocked by zevbox strict mode (the agent runs in an isolated Linux VM)."
    );
    assert_eq!(
        cwd_message("/a", "/b"),
        "zevbox: this agent is bound to /a; this session asked for /b. Open /a in Zed, or run 'zevbox project add /b' in a terminal."
    );
    assert_eq!(
        additional_directories_message(),
        "zevbox: extra workspace folders are not supported; open a single folder."
    );
    assert_eq!(
        auth_hint("claude"),
        " → run 'zevbox auth claude' in a macOS terminal."
    );
}
